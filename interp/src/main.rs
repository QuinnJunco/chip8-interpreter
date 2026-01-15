use std::{fs::{self, File}, process::{exit, id}, sync::*, thread, time::{Duration, Instant}};

use macroquad::prelude::*;

const FILE_NOT_FOUND: i32 = 1;

const FONT: [u8; 80] = [
        0xF0, 0x90, 0x90, 0x90, 0xF0, // 0
        0x20, 0x60, 0x20, 0x20, 0x70, // 1
        0xF0, 0x10, 0xF0, 0x80, 0xF0, // 2
        0xF0, 0x10, 0xF0, 0x10, 0xF0, // 3
        0x90, 0x90, 0xF0, 0x10, 0x10, // 4
        0xF0, 0x80, 0xF0, 0x10, 0xF0, // 5
        0xF0, 0x80, 0xF0, 0x90, 0xF0, // 6
        0xF0, 0x10, 0x20, 0x40, 0x40, // 7
        0xF0, 0x90, 0xF0, 0x90, 0xF0, // 8
        0xF0, 0x90, 0xF0, 0x10, 0xF0, // 9
        0xF0, 0x90, 0xF0, 0x90, 0x90, // A
        0xE0, 0x90, 0xE0, 0x90, 0xE0, // B
        0xF0, 0x80, 0x80, 0x80, 0xF0, // C
        0xE0, 0x90, 0x90, 0x90, 0xE0, // D
        0xF0, 0x80, 0xF0, 0x80, 0xF0, // E
        0xF0, 0x80, 0xF0, 0x80, 0x80  // F
        ];

struct Frame<T> {
    value:      T,
    next:       Option<Box<Frame<T>>>,
}

struct Stack {
    top:        Option<Box<Frame<u16>>>
}

impl Stack {
    pub fn new() -> Self {
        return Self { top: None }
    }

    pub fn push(&mut self, value: u16) {
        self.top = Some(Box::new(Frame {value, next: self.top.take()}));
    }

    pub fn pop(&mut self) -> Option<u16> {
        let r = self.top.take();
        match r {
            Some(mut top) => {
                self.top = top.next.take();
                return Some(top.value);
            },
            _ => return None,
        }
    }

    pub fn peak(&mut self) -> Option<u16> {
        match &self.top {
            Some(top) => return Some(top.value),
            _ => return None,
        }
    }
}

fn init_font() -> [u8; 4096] {
    let mut mem = [0; 4096];
    mem[..FONT.len()].copy_from_slice(&FONT);
    return mem;
}

struct Emulator {
    mem:        [u8; 4096], // main memory
    disp:       [u8; 256], // 64x32 pixel display
    pc:         u16, // program counter
    idx:        u16, // index register
    stack:      Stack, // stack
    delay:      Arc<Mutex<u8>>, // delay timer
    sound:      Arc<Mutex<u8>>, // sound timer
    reg:        [u8; 16], // general purpose registers
}

impl Emulator {
    pub const PROGRAM_START: usize = 0x200;

    pub fn init() -> Self {
        return Self { 
            mem: init_font(), 
            disp: [0; 256], 
            pc: Emulator::PROGRAM_START as u16, 
            idx: 0, 
            stack: Stack::new(), 
            delay: Arc::new(Mutex::new(0)), 
            sound: Arc::new(Mutex::new(0)), 
            reg: [0; 16] 
        };
    }

    pub fn getWord(&self, addr: u16) -> u8 {
        return self.mem[addr as usize];
    }

    pub fn putWord(&mut self, addr: u16, value: u8) {
        self.mem[addr as usize] = value;
    }

    pub fn getDWord(&self, addr: u16) -> u16 {
        let addr = addr as usize;
        let msb: u16 = self.mem[addr] as u16;
        let lsb: u16 = self.mem[addr + 1] as u16;

        return (msb << 8) | lsb;
    }

    pub fn putDWord(&mut self, addr: u16, value: u16) {
        let lsb = (value & 0xff) as u8;
        let msb = ((value >> 8) & 0xff) as u8;
        
        let addr = addr as usize;
        self.mem[addr] = msb;
        self.mem[addr + 1] = lsb;
    }

    pub fn loadROM(&mut self, file_name: &str) {
        match fs::read(file_name) {
            Ok(bytes) => {
                let start_addr = Emulator::PROGRAM_START;
                self.mem[start_addr..start_addr + bytes.len()].copy_from_slice(&bytes);
            }
            Err(_) => {
                println!("Failed to open file: {0}", file_name);
                exit(FILE_NOT_FOUND);
            }
        };
    }

    pub fn readPixel(&self, pixel: u16) -> u8 {
        let px_o = pixel % 8;
        let px_i = ((pixel - px_o) / 8) as usize;
        return (self.disp[px_i] >> px_o) & 0x1// need to shift by px_o unsure how at the moment need to reason out stuff
    }

    pub fn writePixel(&mut self, pixel: u16, value: u8) {
        let px_o = pixel % 8;
        let px_i = ((pixel - px_o) / 8) as usize;
        self.disp[px_i] ^= value << px_o;
    }

    pub fn draw(&self) {
        let w = screen_width()/64.0;
        let h = screen_height()/32.0;

        let screen_width = w * 64.0;
        
        let mut pt = vec2(0.0, 0.0);
        for px in self.disp {
            for b in 0..8 {
                if pt.x == screen_width {
                    pt.x = 0.0;
                    pt.y += h;
                }
                let c = if ((px >> b) & 0x1) == 1 {WHITE} else {BLACK};
                draw_rectangle(pt.x, pt.y, pt.x+w, pt.y+h, c);
                pt.x += w;
                
            }
        }
    }
}

fn tick(delay: Arc<Mutex<u8>>, sound: Arc<Mutex<u8>>) {
    let period = Duration::from_nanos(16_666_667);
    loop {
        let start = Instant::now();

        {
            let mut d = delay.lock().unwrap();
            if *d > 0 {
                *d -= 1;
            }
        }

        {
            let mut s = sound.lock().unwrap();
            if *s > 0 {
                *s -= 1;
            }
        }

        let elapsed = start.elapsed();
        if elapsed < period {
            thread::sleep(period-elapsed);
        }
    }
}

struct Instruction {
    opcode:     u8,
    op1:        Option<u16>,
    op2:        Option<u16>,
    op3:        Option<u16>,
}

/* OPCODE addr */
macro_rules! PARSE_FORMAT_1 {
    ($raw: expr, $instr: expr) => {
        {
            $instr.op1 = Some(($raw & 0xFFF) as u16)
        }
    };
}

/* OPCODE Vx byte */
macro_rules! PARSE_FORMAT_2 {
    ($raw: expr, $instr: expr) => {
        {
            $instr.op1 = Some((($raw >> 8) & 0xF) as u16);
            $instr.op2 = Some(($raw & 0xFF) as u16);
        }
    };
}

/* OPCODE Vx Vy nibble */
macro_rules! PARSE_FORMAT_3 {
    ($raw: expr, $instr: expr) => {
        {
            $instr.op1 = Some((($raw >> 8) & 0xF) as u16);
            $instr.op2 = Some((($raw >> 4) & 0xF) as u16);
            $instr.op3 = Some(($raw  & 0xF) as u16);
        }
    };
}

impl Instruction {
    pub fn new(raw: u16) -> Instruction {
        let mut instr: Instruction = Instruction {
            opcode: ((raw >> 12) & 0xf) as u8,
            op1: None, 
            op2: None, 
            op3: None 
        };
        
        match instr.opcode {
            0x0 | 0x1 | 0x2 | 0xA | 0xB => PARSE_FORMAT_1!(raw, instr),
            0x3 | 0x4 | 0x6 | 0x7 | 0xC | 0xE | 0xF => PARSE_FORMAT_2!(raw, instr),
            0x5 | 0x8 | 0xD => PARSE_FORMAT_3!(raw, instr),
            _ => {}
        }
        return instr;
    }
}

fn fetch(emu: &mut Emulator) -> u16 {
    let instr = emu.getDWord(emu.pc);
    emu.pc += 2;
    return instr;
}

fn decode(dword: u16) -> Instruction {
    return Instruction::new(dword);
}

fn execute(emu: &mut Emulator, instr: Instruction) {
    match instr.opcode {
        0x0 => {
            match instr.op1 {
                Some(0x0E0) => {
                    emu.disp.fill(0x00);
                    emu.draw();
                }
                Some(0x0EE) => {
                    match emu.stack.pop() {
                        Some(addr) => {
                            emu.pc = addr;
                        }
                        _ => println!("ERROR: Failed to pop from stack, stack empty.")
                    }
                }
                _ => println!("ERROR: Unknown operand for opcode: {:x}", instr.opcode)
            }
        }
        0x1 => {
            match instr.op1 {
                Some(n) => {
                    emu.pc = n;
                }
                _ => println!("ERROR: Unknown operand for opcode: {:x}", instr.opcode)
            }
        }
        0x2 => {
            match instr.op1 {
                Some(n) => {
                    emu.stack.push(emu.pc);
                    emu.pc = n;
                }
                _ => println!("ERROR: Unknown operand for opcode: {:x}", instr.opcode)
            }
        }
        0x3 => {
            match (instr.op1, instr.op2) {
                (Some(x), Some(b)) => {
                    if emu.reg[x as usize] == (b as u8) {
                        emu.pc += 2;
                    }
                }
                _ => println!("ERROR: Unknown operand for opcode: {:x}", instr.opcode)
            }
        }
        0x4 => {
            match (instr.op1, instr.op2) {
                (Some(x), Some(b)) => {
                    if emu.reg[x as usize] != (b as u8) {
                        emu.pc += 2;
                    }
                }
                _ => println!("ERROR: Unknown operand for opcode: {:x}", instr.opcode)
            }
        }
        0x5 => {
            match (instr.op1, instr.op2) {
                (Some(x), Some(y)) => {
                    if emu.reg[x as usize] == emu.reg[y as usize] {
                        emu.pc += 2;
                    }
                }
                _ => println!("ERROR: Unknown operand for opcode: {:x}", instr.opcode)
            }
        }
        0x6 => {
            match (instr.op1, instr.op2) {
                (Some(x), Some(b)) => {
                    emu.reg[x as usize] = b as u8;
                }
                _ => println!("ERROR: Unknown operand for opcode: {:x}", instr.opcode)
            }
        }
        0x7 => {
            match (instr.op1, instr.op2) {
                (Some(x), Some(b)) => {
                    let x = x as usize;
                    emu.reg[x] = emu.reg[x] + (b as u8);
                }
                _ => println!("ERROR: Unknown operand for opcode: {:x}", instr.opcode)
            }
        }
        0x8 => {
            todo!();
        }
        0x9 => {
            todo!();
        }
        0xA => {
            match instr.op1 {
                Some(n) => {
                    emu.idx = n;
                }
                _ => println!("ERROR: Unknown operand for opcode: {:x}", instr.opcode)
            }
        }
        0xB => {
            todo!();
        }
        0xC => {
            todo!();
        }
        0xD => {
            match (instr.op1, instr.op2, instr.op3) {
                (Some(x ), Some(y), Some(n)) => {
                    let n = n as u8;
                    let mut sprite: [u8; 0xF] = [0; 0xF];
                    for i in 0..n {
                        sprite[i as usize] = emu.getWord(emu.idx + (i as u16));
                    }
                    
                    let Vx = emu.reg[x as usize] as u16;
                    let Vy = emu.reg[y as usize] as u16;
                    println!("({0}, {1})", Vx, Vy);
                    
                    let px = Vx + (Vy * 32);
                    let index = (px / 8) as usize;
                    let offset = (px % 8);
                    println!("px={0}, index={1}, offset={2}", px, index, offset);
                    let mut VF = 0;
                    for i in 0..n {
                        let i = i as usize;
                        sprite[i] ^= emu.disp[index + i];
                        if sprite[i] != (sprite[i] & emu.disp[index + i]) {VF = 1};
                        emu.disp[index + i] = sprite[i];
                    }
                    emu.reg[0xF] = VF;
                }
                _ => println!("ERROR: Unknown operand for opcode: {:x}", instr.opcode)
            }
        }
        0xE => {
            todo!();
        }
        0xF => {
            todo!();
        }
        x => println!("Unknown opcode: {:x}.", x)
    }
}


#[macroquad::main("Chip8")]
async fn main() {
    let mut emu = Emulator::init();
    let delay = Arc::clone(&emu.delay);
    let sound = Arc::clone(&emu.sound);

    thread::spawn(move || tick(delay, sound));

    emu.loadROM("./roms/IBM Logo.ch8");
    emu.disp[0] = 0b10000001;
    println!("idx0={0} idx7={1}", emu.readPixel(0), emu.readPixel(7));

    let mut i = 0;
    loop {
        if i == 10 {
            emu.writePixel(0, 1);
        }
        if i == 250 {
            emu.writePixel(0, 1);
        }
        // emu.disp[i%256] ^= 0xFF; 
        
        // let raw = fetch(&mut emu);
        // let instr = decode(raw);
        // execute(&mut emu, instr);
        
        emu.draw();
        next_frame().await;
        
        i += 1;
        i %= 256;
        
    }
}
