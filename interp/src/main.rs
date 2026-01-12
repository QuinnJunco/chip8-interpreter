use std::{fs::{self, File}, process::{exit, id}, sync::*, thread, time::{Duration, Instant}};

use macroquad::prelude::*;

const FILE_NOT_FOUND: i32 = 1;

enum Register {
    V0=0,
    V1,
    V2,
    V3,
    V4,
    V5,
    V6,
    V7,
    V8,
    V9,
    VA,
    VB,
    VC,
    VD,
    VE,
    VF,
}

use Register::*;

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
    let font = [
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

    let mut mem = [0; 4096];
    
    mem[..font.len()].copy_from_slice(&font);

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
            pc: 0x200, 
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
            $instr.op2 = Some(($raw  & 0xF) as u16);
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
            0 | 1 | 2 | 0xA | 0xB => PARSE_FORMAT_1!(raw, instr),
            3 | 4 | 6 | 7 | 0xC | 0xE | 0xF => PARSE_FORMAT_2!(raw, instr),
            5 | 8 | 0xD => PARSE_FORMAT_3!(raw, instr),
            x => println!("Unknown opcode: {:x}.", x)
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
    println!("OPCODE: {:x}", instr.opcode);
}

fn main() {
    let mut emu = Emulator::init();
    let delay = Arc::clone(&emu.delay);
    let sound = Arc::clone(&emu.sound);

    thread::spawn(move || tick(delay, sound));

    emu.loadROM("./roms/IBM Logo.ch8");
    
    for i in 0..10 {
        let raw = fetch(&mut emu);
        let instr = decode(raw);
        execute(&mut emu, instr);
    }
    
}
