use oxidz80::OxidZ80;
use oxide_core::{Cpu, MemoryBus};
use serde::Deserialize;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

#[derive(Deserialize, Debug)]
struct TestState {
    pc: u16,
    sp: u16,
    a: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
    f: u8,
    h: u8,
    l: u8,
    i: u8,
    r: u8,
    ix: u16,
    iy: u16,
    #[serde(rename = "wz")]
    _wz: u16, // Internal register, can ignore for now or check if we expose it
    #[serde(rename = "af_")]
    af_prime: u16,
    #[serde(rename = "bc_")]
    bc_prime: u16,
    #[serde(rename = "de_")]
    de_prime: u16,
    #[serde(rename = "hl_")]
    hl_prime: u16,
    ram: Vec<(u16, u8)>,
}

#[derive(Deserialize, Debug)]
struct TestCase {
    name: String,
    initial: TestState,
    #[serde(rename = "final")]
    final_state: TestState,
    cycles: Vec<(u16, u16, String)>, // Addr, Data, Type (read/write) - Not fully checking yet
}

struct TestBus {
    memory: [u8; 65536],
}

impl TestBus {
    fn new(ram: &[(u16, u8)]) -> Self {
        let mut bus = Self { memory: [0; 65536] };
        for &(addr, val) in ram {
            bus.memory[addr as usize] = val;
        }
        bus
    }
}

impl MemoryBus for TestBus {
    fn read(&self, addr: u32) -> u8 {
        self.memory[(addr & 0xFFFF) as usize]
    }

    fn write(&mut self, addr: u32, value: u8) {
        self.memory[(addr & 0xFFFF) as usize] = value;
    }

    fn port_in(&mut self, _port: u16) -> u8 { 0xFF } // Dummy I/O
    fn port_out(&mut self, _port: u16, _value: u8) {}
}

const TESTS_DIR: &str = "../../tests/z80_json_tests"; // Adjust path as needed

#[test]
#[ignore] // Start ignored until test files are present
fn run_z80_json_tests() {
    let path = Path::new(TESTS_DIR);
    if !path.exists() {
        println!("Test directory not found: {:?}", path);
        return;
    }

    let mut total_tests = 0;
    let mut passed_tests = 0;

    for entry in std::fs::read_dir(path).expect("Read dir failed") {
        let entry = entry.expect("Entry failed");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            println!("Running tests from {:?}", path.file_name().unwrap());
            let file = File::open(&path).expect("File open failed");
            let reader = BufReader::new(file);
            let tests: Vec<TestCase> = serde_json::from_reader(reader).expect("JSON parse failed");

            for test in tests {
                total_tests += 1;
                if run_single_test(&test) {
                    passed_tests += 1;
                } else {
                    panic!("Test failed: {}", test.name);
                }
            }
        }
    }
    
    println!("Passed {} / {} tests", passed_tests, total_tests);
}

fn run_single_test(test: &TestCase) -> bool {
    // 1. Setup Bus
    let mut bus = TestBus::new(&test.initial.ram);

    // 2. Setup CPU
    let mut cpu = OxidZ80::new();
    cpu.pc = test.initial.pc;
    cpu.sp = test.initial.sp;
    cpu.a = test.initial.a;
    cpu.b = test.initial.b;
    cpu.c = test.initial.c;
    cpu.d = test.initial.d;
    cpu.e = test.initial.e;
    cpu.f = test.initial.f;
    cpu.h = test.initial.h;
    cpu.l = test.initial.l;
    cpu.i = test.initial.i;
    cpu.r = test.initial.r;
    cpu.ix = test.initial.ix;
    cpu.iy = test.initial.iy;
    
    // Set shadow regs (Assuming internal storage in OxidZ80 maps to standard pairs)
    // NOTE: OxidZ80 needs methods or public fields to set prime registers directly if they aren't exposed.
    // For now assuming we can set them via internal modification or helper? 
    // OxidZ80 defines: `a_p`, `f_p`, `bc_p`, `de_p`, `hl_p`. (Check `lib.rs`)
    // We might need to make these public or add a `set_state` method.
    // Let's assume for now we need to modify `oxidz80` to be testable or use `unsafe` / struct access if pub.
    // Checking `oxidz80/src/lib.rs` -> fields are likely private.
    // We will need to update `lib.rs` to expose a `set_state_for_test` or make fields pub crate.
    
    // Hack: For this generation, I'll assume I can modify `lib.rs` to make fields public or add a setter.
    // I will write `set_internal_state` method in `lib.rs` next.
    cpu.set_internals(
        test.initial.af_prime,
        test.initial.bc_prime,
        test.initial.de_prime,
        test.initial.hl_prime,
        test.initial._wz // MemPtr
    );

    // 3. Step
    let _cycles = cpu.step(&mut bus);

    // 4. Verify
    let mut ok = true;
    if cpu.pc != test.final_state.pc { println!("PC mismatch: {:04X} != {:04X}", cpu.pc, test.final_state.pc); ok = false; }
    if cpu.sp != test.final_state.sp { println!("SP mismatch: {:04X} != {:04X}", cpu.sp, test.final_state.sp); ok = false; }
    if cpu.a != test.final_state.a { println!("A mismatch: {:02X} != {:02X}", cpu.a, test.final_state.a); ok = false; }
    if cpu.f != test.final_state.f { println!("F mismatch: {:02X} != {:02X}", cpu.f, test.final_state.f); ok = false; }
    if cpu.b != test.final_state.b { println!("B mismatch"); ok = false; }
    if cpu.c != test.final_state.c { println!("C mismatch"); ok = false; }
    if cpu.d != test.final_state.d { println!("D mismatch"); ok = false; }
    if cpu.e != test.final_state.e { println!("E mismatch"); ok = false; }
    if cpu.h != test.final_state.h { println!("H mismatch"); ok = false; }
    if cpu.l != test.final_state.l { println!("L mismatch"); ok = false; }
    if cpu.ix != test.final_state.ix { println!("IX mismatch"); ok = false; }
    if cpu.iy != test.final_state.iy { println!("IY mismatch"); ok = false; }
    
    // Verify RAM
    for (addr, val) in &test.final_state.ram {
        let mem_val = bus.memory[*addr as usize];
        if mem_val != *val {
            println!("RAM mismatch at {:04X}: {:02X} != {:02X}", addr, mem_val, val);
            ok = false;
        }
    }

    ok
}
