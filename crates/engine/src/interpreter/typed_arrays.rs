//! Typed Arrays + DataView - SharedArrayBuffer + atomics primitives.
//!
//! ECMA-262 25.1+.
//! 11 typed array kinds (Int8/Uint8/Uint8Clamped/Int16/Uint16/Int32/Uint32/Float16/Float32/Float64/BigInt64/BigUint64).

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TypedArrayKind {
    Int8, Uint8, Uint8Clamped,
    Int16, Uint16,
    Int32, Uint32,
    Float16, Float32, Float64,
    BigInt64, BigUint64,
}

impl TypedArrayKind {
    pub fn element_size(&self) -> usize {
        match self {
            Self::Int8 | Self::Uint8 | Self::Uint8Clamped => 1,
            Self::Int16 | Self::Uint16 | Self::Float16 => 2,
            Self::Int32 | Self::Uint32 | Self::Float32 => 4,
            Self::Float64 | Self::BigInt64 | Self::BigUint64 => 8,
        }
    }

    pub fn ctor_name(&self) -> &'static str {
        match self {
            Self::Int8 => "Int8Array",
            Self::Uint8 => "Uint8Array",
            Self::Uint8Clamped => "Uint8ClampedArray",
            Self::Int16 => "Int16Array",
            Self::Uint16 => "Uint16Array",
            Self::Int32 => "Int32Array",
            Self::Uint32 => "Uint32Array",
            Self::Float16 => "Float16Array",
            Self::Float32 => "Float32Array",
            Self::Float64 => "Float64Array",
            Self::BigInt64 => "BigInt64Array",
            Self::BigUint64 => "BigUint64Array",
        }
    }

    pub fn from_ctor(name: &str) -> Option<Self> {
        Some(match name {
            "Int8Array" => Self::Int8,
            "Uint8Array" => Self::Uint8,
            "Uint8ClampedArray" => Self::Uint8Clamped,
            "Int16Array" => Self::Int16,
            "Uint16Array" => Self::Uint16,
            "Int32Array" => Self::Int32,
            "Uint32Array" => Self::Uint32,
            "Float16Array" => Self::Float16,
            "Float32Array" => Self::Float32,
            "Float64Array" => Self::Float64,
            "BigInt64Array" => Self::BigInt64,
            "BigUint64Array" => Self::BigUint64,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ArrayBuffer {
    pub bytes: Vec<u8>,
    pub max_byte_length: Option<usize>,    // resizable buffer (TC39 stage 3)
    pub detached: bool,
}

impl ArrayBuffer {
    pub fn new(size: usize) -> Self {
        Self { bytes: vec![0; size], max_byte_length: None, detached: false }
    }

    pub fn resize(&mut self, new_size: usize) -> Result<(), String> {
        let Some(max) = self.max_byte_length else { return Err("non-resizable".into()); };
        if new_size > max { return Err("exceeds max_byte_length".into()); }
        self.bytes.resize(new_size, 0);
        Ok(())
    }

    pub fn detach(&mut self) -> Vec<u8> {
        self.detached = true;
        std::mem::take(&mut self.bytes)
    }
}

#[derive(Debug, Clone)]
pub struct TypedArrayView {
    pub buffer_index: usize,        // pointer into a buffer registry
    pub byte_offset: usize,
    pub length: usize,              // in elements
    pub kind: TypedArrayKind,
}

impl TypedArrayView {
    pub fn byte_length(&self) -> usize {
        self.length * self.kind.element_size()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Endian {
    Little,
    Big,
}

/// DataView accessor: read element from buffer at byte offset.
pub fn data_view_read_u16(buf: &[u8], offset: usize, endian: Endian) -> Option<u16> {
    if offset + 2 > buf.len() { return None; }
    let bytes = [buf[offset], buf[offset + 1]];
    Some(match endian {
        Endian::Little => u16::from_le_bytes(bytes),
        Endian::Big => u16::from_be_bytes(bytes),
    })
}

pub fn data_view_read_u32(buf: &[u8], offset: usize, endian: Endian) -> Option<u32> {
    if offset + 4 > buf.len() { return None; }
    let bytes = [buf[offset], buf[offset + 1], buf[offset + 2], buf[offset + 3]];
    Some(match endian {
        Endian::Little => u32::from_le_bytes(bytes),
        Endian::Big => u32::from_be_bytes(bytes),
    })
}

pub fn data_view_read_f32(buf: &[u8], offset: usize, endian: Endian) -> Option<f32> {
    let bits = data_view_read_u32(buf, offset, endian)?;
    Some(f32::from_bits(bits))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_sizes() {
        assert_eq!(TypedArrayKind::Int8.element_size(), 1);
        assert_eq!(TypedArrayKind::Float32.element_size(), 4);
        assert_eq!(TypedArrayKind::BigInt64.element_size(), 8);
    }

    #[test]
    fn ctor_name_lookup() {
        let k = TypedArrayKind::from_ctor("Uint8ClampedArray").unwrap();
        assert_eq!(k.ctor_name(), "Uint8ClampedArray");
    }

    #[test]
    fn buffer_resize_under_max() {
        let mut b = ArrayBuffer::new(10);
        b.max_byte_length = Some(20);
        assert!(b.resize(15).is_ok());
        assert_eq!(b.bytes.len(), 15);
    }

    #[test]
    fn buffer_resize_over_max_errors() {
        let mut b = ArrayBuffer::new(10);
        b.max_byte_length = Some(20);
        assert!(b.resize(25).is_err());
    }

    #[test]
    fn detach_returns_buffer() {
        let mut b = ArrayBuffer::new(5);
        let bytes = b.detach();
        assert_eq!(bytes.len(), 5);
        assert!(b.detached);
    }

    #[test]
    fn dataview_read_u16_le() {
        let buf = [0x34, 0x12, 0xff, 0xff];
        assert_eq!(data_view_read_u16(&buf, 0, Endian::Little), Some(0x1234));
        assert_eq!(data_view_read_u16(&buf, 0, Endian::Big), Some(0x3412));
    }

    #[test]
    fn dataview_oob_returns_none() {
        let buf = [0u8; 2];
        assert!(data_view_read_u32(&buf, 0, Endian::Little).is_none());
    }

    #[test]
    fn typed_array_byte_length() {
        let v = TypedArrayView {
            buffer_index: 0, byte_offset: 0, length: 10, kind: TypedArrayKind::Float32,
        };
        assert_eq!(v.byte_length(), 40);
    }
}
