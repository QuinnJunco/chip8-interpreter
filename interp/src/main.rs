use std::{fs::{self, File}, process::{exit, id}, sync::*, thread, time::{Duration, Instant}};

use macroquad::{audio::{Sound, play_sound}, prelude::*, rand::{self, rand}};

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

const TOTAL_PIXELS: u16 = 64 * 32;

const REAL_KEYCODES: [KeyCode; 16] = [
    KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4,
    KeyCode::Q, KeyCode::W, KeyCode::E, KeyCode::R,
    KeyCode::A, KeyCode::S, KeyCode::D, KeyCode::F,
    KeyCode::Z, KeyCode::X, KeyCode::C, KeyCode::V
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

    pub fn writePixel(&mut self, pixel: u16, value: u8) -> u8 {
        let px_o = pixel % 8;
        let px_i = (((pixel - px_o) / 8) % TOTAL_PIXELS) as usize;

        let old = self.disp[px_i];
        self.disp[px_i] ^= value << px_o;

        return if old == self.disp[px_i] {1} else {0};
    }

    pub fn draw(&self) {
        let w = screen_width()/64.0;
        let h = screen_height()/32.0;
        let screen_width = w * 64.0;
        let mut pt = vec2(0.0, 0.0);
        for i in 0..TOTAL_PIXELS {
            if pt.x == screen_width {
                pt.x = 0.0;
                pt.y += h;
            }
            let c = if ((self.readPixel(i))) == 1 {WHITE} else {BLACK};
            draw_rectangle(pt.x, pt.y, pt.x+w, pt.y+h, c);
            pt.x += w;
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
            match (instr.op1, instr.op2, instr.op3) {
                (Some(x), Some(y), Some(0x0)) => {
                    emu.reg[x as usize] = emu.reg[y as usize];
                }
                (Some(x), Some(y), Some(0x1)) => {
                    emu.reg[x as usize] |= emu.reg[y as usize];
                }
                (Some(x), Some(y), Some(0x2)) => {
                    emu.reg[x as usize] &= emu.reg[y as usize];
                }
                (Some(x), Some(y), Some(0x3)) => {
                    emu.reg[x as usize] ^= emu.reg[y as usize];
                }
                (Some(x), Some(y), Some(0x4)) => {
                    let x = x as usize;
                    let sum = (emu.reg[x] as u16) + (emu.reg[y as usize] as u16);
                    if sum > 0xff {
                        emu.reg[0xf] = 1;
                        emu.reg[x] = 0xff;
                    } else {
                        emu.reg[x] = sum as u8;
                    }
                }
                (Some(x), Some(y), Some(0x5)) => {
                    let x = x as usize;
                    let y = y as usize;

                    emu.reg[0xf] = if emu.reg[x] > emu.reg[y] {1} else {0};
                    emu.reg[x] -= emu.reg[y];
                }
                (Some(x), Some(_), Some(0x6)) => {
                    let x = x as usize;
                    emu.reg[0xf] = if emu.reg[x] & 0x1 == 1 {1} else {0};
                    emu.reg[x] /= 2;
                }
                (Some(x), Some(y), Some(0x7)) => {
                    let x = x as usize;
                    let y = y as usize;

                    emu.reg[0xf] = if emu.reg[x] < emu.reg[y] {1} else {0};
                    emu.reg[x] = emu.reg[y] - emu.reg[x];
                }
                (Some(x), Some(_), Some(0xE)) => {
                    let x = x as usize;
                    emu.reg[0xf] = if emu.reg[x] >> 7 & 0x1 == 1 {1} else {0};
                    emu.reg[x] *= 2;
                }
                _ => println!("ERROR: Unknown operand for opcode: {:x}", instr.opcode)
            }
        }
        0x9 => {
            match (instr.op1, instr.op2) {
                (Some(x), Some(y)) => {
                    if emu.reg[x as usize] != emu.reg[y as usize] {
                        emu.pc += 2;
                    }
                }
                _ => println!("ERROR: Unknown operand for opcode: {:x}", instr.opcode)
            }
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
            match instr.op1 {
                Some(n) => {
                    emu.pc = (emu.reg[0] as u16) + n;
                }
                _ => println!("ERROR: Unknown operand for opcode: {:x}", instr.opcode)
            }
        }
        0xC => {
            match (instr.op1, instr.op2) {
                (Some(x), Some(n) ) => {
                    emu.reg[x as usize] = (n as u8) & ((rand() & 0xff) as u8);
                }
                _ => println!("ERROR: Unknown operand for opcode: {:x}", instr.opcode)
            }
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
                    
                    let mut px = Vx + (Vy * 32);
                
                    let mut VF = 0;
                    for s in 0..n {
                        for o in 0..8 {
                            if emu.writePixel(px + o, (sprite[s as usize] >> (7 - o)) & 0x1) == 1 {VF = 1;}
                        }
                        px += 64;
                        if px >= TOTAL_PIXELS {break;}
                    }
                    emu.reg[0xF] = VF;
                }
                _ => println!("ERROR: Unknown operand for opcode: {:x}", instr.opcode)
            }
        }
        0xE => {
            match (instr.op1, instr.op2) {
                (Some(x), Some(0x9E)) => {
                    if is_key_down(REAL_KEYCODES[emu.reg[x as usize] as usize]) {
                        emu.pc += 2;
                    }
                }
                (Some(x), Some(0xA1)) => {
                    if !is_key_down(REAL_KEYCODES[emu.reg[x as usize] as usize]) {
                        emu.pc += 2;
                    }
                }
                _ => println!("ERROR: Unknown operand for opcode: {:x}", instr.opcode)
            }
            
        }
        0xF => {
            match (instr.op1, instr.op2) {
                (Some(x), Some(0x07)) => {
                    {
                        let d = emu.delay.lock().unwrap();
                        emu.reg[x as usize] = *d;
                    }
                }
                (Some(x), Some(0x0A)) => {
                    let x = x as usize;
                    let mut pressed: bool = false;
                    for i in 0..REAL_KEYCODES.len() {
                        if is_key_down(REAL_KEYCODES[i]) {
                            emu.reg[x] = i as u8;
                            pressed = true;
                            break;
                        }
                    }

                    if !pressed {
                        emu.pc -= 2;
                    }
                }
                (Some(x), Some(0x15)) => {
                    {
                        let mut d = emu.delay.lock().unwrap();
                        *d = emu.reg[x as usize];
                    }
                }
                (Some(x), Some(0x18)) => {
                    {
                        let mut s = emu.sound.lock().unwrap();
                        *s = emu.reg[x as usize];
                    }

                }
                (Some(x), Some(0x1E)) => {
                    emu.idx += emu.reg[x as usize] as u16;
                }
                (Some(x), Some(0x29)) => {
                    emu.idx = (emu.reg[x as usize] * 5) as u16;
                }
                (Some(x), Some(0x33)) => {
                    let x = x as usize;
                    let idx = emu.idx as usize;
                    emu.mem[idx] = (emu.reg[x] / 100) % 10;
                    emu.mem[idx + 1] = (emu.reg[x] / 10) % 10;
                    emu.mem[idx + 2] = emu.reg[x] % 10;
                }
                (Some(x), Some(0x55)) => {
                    let mut idx = emu.idx as usize;
                    for i in 0..x {
                        let i = i as usize;
                        emu.mem[idx] = emu.reg[i];
                        idx += 1;
                    }

                }
                (Some(x), Some(0x65)) => {
                    let mut idx = emu.idx as usize;
                    for i in 0..x {
                        let i = i as usize;
                        emu.reg[i] = emu.mem[idx];
                        idx += 1;
                    }
                }
                _ => println!("ERROR: Unknown operand for opcode: {:x}", instr.opcode)
            }
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

    loop {
        let raw = fetch(&mut emu);
        let instr = decode(raw);
        execute(&mut emu, instr);
        
        emu.draw();
        next_frame().await;
    }
}
