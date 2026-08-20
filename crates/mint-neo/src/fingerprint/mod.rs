use crate::abi::Endianness;
use crate::layout::ResolvedLayout;
use crate::types::{TypeId, TypeKind};

const HASH_CONTEXT: &str = "mint neo block ABI fingerprint v1";

pub fn calculate(layout: &ResolvedLayout) -> u64 {
    let mut hasher = blake3::Hasher::new_derive_key(HASH_CONTEXT);
    hasher.update(&[match layout.abi.endianness() {
        Endianness::Little => 0,
        Endianness::Big => 1,
    }]);
    hash_u64(layout.abi.address_unit_bits() as u64, &mut hasher);
    hash_type(layout, layout.root, &mut hasher);
    let digest = hasher.finalize();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest.as_bytes()[..8]);
    u64::from_be_bytes(bytes)
}

fn hash_type(layout: &ResolvedLayout, id: TypeId, hasher: &mut blake3::Hasher) {
    let resolved = &layout.layouts[id.0];
    match &layout.types[id.0] {
        TypeKind::Scalar { scalar, .. } => {
            hasher.update(&[0]);
            hasher.update(&[scalar.hash_tag()]);
            hash_u64(resolved.size as u64, hasher);
            hash_u64(resolved.alignment as u64, hasher);
            if let Some((_, scalar_abi)) = resolved.scalar {
                hash_u64(scalar_abi.storage_size as u64, hasher);
                hash_u64(scalar_abi.alignment as u64, hasher);
                hash_u64(scalar_abi.array_stride as u64, hasher);
            }
        }
        TypeKind::Record { .. } => {
            hasher.update(&[1]);
            hash_u64(resolved.size as u64, hasher);
            hash_u64(resolved.alignment as u64, hasher);
            hash_u64(resolved.fields.len() as u64, hasher);
            for field in &resolved.fields {
                hash_u64(field.offset as u64, hasher);
                hash_u64(field.size as u64, hasher);
                hash_u64(field.alignment as u64, hasher);
                hash_type(layout, field.type_id, hasher);
            }
        }
        TypeKind::Array { .. } => {
            hasher.update(&[2]);
            hash_u64(resolved.size as u64, hasher);
            hash_u64(resolved.alignment as u64, hasher);
            if let Some(array) = &resolved.array {
                hash_u64(array.dimensions.len() as u64, hasher);
                for dim in &array.dimensions {
                    hash_u64(*dim, hasher);
                }
                hash_u64(array.stride as u64, hasher);
                hash_type(layout, array.element, hasher);
            }
        }
        TypeKind::Enum => {
            hasher.update(&[3]);
        }
    }
}

fn hash_u64(value: u64, hasher: &mut blake3::Hasher) {
    hasher.update(&value.to_le_bytes());
}

pub fn hex(value: u64) -> String {
    format!("{value:016x}")
}
