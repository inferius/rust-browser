//! WebAssembly JS API foundation - `WebAssembly.instantiate`, Module, Memory, Table.
//!
//! Real impl by integrate `wasmtime` ci `wasmer` crate. Foundation = struct
//! definitions + API surface bez actual wasm execution.
//!
//! Inspired by Chromium V8 WASM integration + spec
//! https://webassembly.github.io/spec/js-api/.

use std::rc::Rc;
use std::cell::RefCell;

#[derive(Debug, Clone)]
pub struct WasmModule {
    pub bytes: Vec<u8>,
    /// Parsed type imports (name + type) - real impl pres wasmparser.
    pub imports: Vec<WasmImport>,
    pub exports: Vec<WasmExport>,
}

#[derive(Debug, Clone)]
pub struct WasmImport {
    pub module_name: String,
    pub field_name: String,
    pub kind: WasmExternKind,
}

#[derive(Debug, Clone)]
pub struct WasmExport {
    pub name: String,
    pub kind: WasmExternKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WasmExternKind {
    Function,
    Table,
    Memory,
    Global,
}

/// WebAssembly.Memory - linear memory accessible jako ArrayBuffer.
#[derive(Debug)]
pub struct WasmMemory {
    pub initial_pages: u32,
    pub maximum_pages: Option<u32>,
    /// Underlying byte buffer - real impl pres mmap'd region.
    pub buffer: RefCell<Vec<u8>>,
}

impl WasmMemory {
    pub fn new(initial_pages: u32, maximum_pages: Option<u32>) -> Self {
        let size = initial_pages as usize * 65536; // 1 page = 64KB
        Self {
            initial_pages,
            maximum_pages,
            buffer: RefCell::new(vec![0u8; size]),
        }
    }

    /// Grow memory by N pages. Vraci previous page count nebo -1 pri fail.
    pub fn grow(&self, delta_pages: u32) -> i32 {
        let mut buf = self.buffer.borrow_mut();
        let cur_pages = (buf.len() / 65536) as u32;
        let new_pages = cur_pages + delta_pages;
        if let Some(max) = self.maximum_pages {
            if new_pages > max { return -1; }
        }
        buf.resize(new_pages as usize * 65536, 0);
        cur_pages as i32
    }

    pub fn byte_length(&self) -> usize {
        self.buffer.borrow().len()
    }
}

#[derive(Debug)]
pub struct WasmTable {
    pub element_type: String, // "anyfunc" / "externref"
    pub initial: u32,
    pub maximum: Option<u32>,
    pub elements: RefCell<Vec<Option<u32>>>, // function indices nebo None
}

impl WasmTable {
    pub fn new(element_type: &str, initial: u32, maximum: Option<u32>) -> Self {
        Self {
            element_type: element_type.to_string(),
            initial,
            maximum,
            elements: RefCell::new(vec![None; initial as usize]),
        }
    }

    pub fn length(&self) -> u32 {
        self.elements.borrow().len() as u32
    }

    pub fn grow(&self, delta: u32) -> i32 {
        let mut els = self.elements.borrow_mut();
        let cur = els.len() as u32;
        let new_len = cur + delta;
        if let Some(max) = self.maximum {
            if new_len > max { return -1; }
        }
        els.resize(new_len as usize, None);
        cur as i32
    }
}

#[derive(Debug)]
pub struct WasmInstance {
    pub module: Rc<WasmModule>,
    pub memory: Option<Rc<WasmMemory>>,
    pub table: Option<Rc<WasmTable>>,
    pub exports: std::collections::HashMap<String, WasmExternKind>,
}

/// Parse WASM module header validitu - real impl pres wasmparser crate.
/// Foundation: check magic bytes + version.
pub fn parse_module_header(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < 8 {
        return Err("WASM module too short".into());
    }
    // Magic: 0x00 0x61 0x73 0x6D
    if &bytes[..4] != b"\0asm" {
        return Err("WASM magic bytes mismatch".into());
    }
    // Version: 0x01 0x00 0x00 0x00
    if &bytes[4..8] != [1, 0, 0, 0] {
        return Err("WASM version unsupported".into());
    }
    Ok(())
}

/// Real WebAssembly instantiate pres `wasmi` interpreter (pure-Rust).
/// Bez system deps, browser sam interpretuje WASM.
///
/// Vraci (engine, store, instance) trojici pro pozdejsi function calls.
/// Engine + Store musi zit za zivotem Instance (wasmi ownership chain).
pub struct RealWasmInstance {
    pub engine: wasmi::Engine,
    pub store: wasmi::Store<()>,
    pub instance: wasmi::Instance,
}

/// Instantiate WASM bytecode pres wasmi. Vraci RealWasmInstance + export names.
pub fn instantiate_real(bytes: &[u8]) -> Result<RealWasmInstance, String> {
    let engine = wasmi::Engine::default();
    let module = wasmi::Module::new(&engine, bytes)
        .map_err(|e| format!("WASM module parse failed: {}", e))?;
    let mut store: wasmi::Store<()> = wasmi::Store::new(&engine, ());
    let linker: wasmi::Linker<()> = wasmi::Linker::new(&engine);
    let pre_instance = linker.instantiate(&mut store, &module)
        .map_err(|e| format!("WASM instantiate failed: {}", e))?;
    let instance = pre_instance.start(&mut store)
        .map_err(|e| format!("WASM start failed: {}", e))?;
    Ok(RealWasmInstance { engine, store, instance })
}

/// Call exported wasm function s i32 args -> i32 vysledek.
/// Pro generic JS-WASM call (BigInt/float/string) pouzij wasmi typed API rozsireni.
pub fn call_export_i32(
    real: &mut RealWasmInstance,
    name: &str,
    args: &[i32],
) -> Result<i32, String> {
    let func = real.instance.get_func(&real.store, name)
        .ok_or_else(|| format!("WASM export '{}' not found", name))?;
    let inputs: Vec<wasmi::Val> = args.iter().map(|v| wasmi::Val::I32(*v)).collect();
    let mut outputs = vec![wasmi::Val::I32(0)];
    func.call(&mut real.store, &inputs, &mut outputs)
        .map_err(|e| format!("WASM call '{}' failed: {}", name, e))?;
    match outputs.first() {
        Some(wasmi::Val::I32(v)) => Ok(*v),
        _ => Err("WASM function did not return i32".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_grow() {
        let m = WasmMemory::new(1, Some(10));
        assert_eq!(m.byte_length(), 65536);
        assert_eq!(m.grow(2), 1); // prev pages = 1
        assert_eq!(m.byte_length(), 65536 * 3);
    }

    #[test]
    fn memory_grow_blocked_by_max() {
        let m = WasmMemory::new(1, Some(2));
        assert_eq!(m.grow(5), -1); // 1 + 5 > 2 max
    }

    #[test]
    fn instantiate_real_minimal_module() {
        // Minimal WASM module: exports `add` fn (i32, i32) -> i32 ktery vrati sum.
        // Bytecode pres wabt: (module (func (export "add") (param i32 i32) (result i32) local.get 0 local.get 1 i32.add))
        let bytes: &[u8] = &[
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // magic + version
            // type section: (i32, i32) -> i32
            0x01, 0x07, 0x01, 0x60, 0x02, 0x7f, 0x7f, 0x01, 0x7f,
            // function section: 1 fn s typem index 0
            0x03, 0x02, 0x01, 0x00,
            // export section: "add" -> func 0
            0x07, 0x07, 0x01, 0x03, 0x61, 0x64, 0x64, 0x00, 0x00,
            // code section: fn body = local.get 0; local.get 1; i32.add; end
            0x0a, 0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6a, 0x0b,
        ];
        let mut real = instantiate_real(bytes).expect("instantiate failed");
        let result = call_export_i32(&mut real, "add", &[5, 7]).expect("call failed");
        assert_eq!(result, 12);
    }

    #[test]
    fn instantiate_real_bad_module_errors() {
        let bytes = [0u8; 10];
        assert!(instantiate_real(&bytes).is_err());
    }

    #[test]
    fn table_grow() {
        let t = WasmTable::new("anyfunc", 5, Some(20));
        assert_eq!(t.length(), 5);
        assert_eq!(t.grow(10), 5);
        assert_eq!(t.length(), 15);
    }

    #[test]
    fn header_validation() {
        assert!(parse_module_header(b"\0asm\x01\0\0\0").is_ok());
        assert!(parse_module_header(b"bad!\0\0\0\0").is_err());
        assert!(parse_module_header(b"\0asm\x02\0\0\0").is_err());
        assert!(parse_module_header(b"short").is_err());
    }
}
