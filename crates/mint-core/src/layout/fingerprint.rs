use super::abi::Endianness;
use super::entry::{EntrySource, RefSource, RefTarget, SizeSource};
use super::error::LayoutError;
use super::resolved::{ResolvedLayout, ResolvedNode, TargetKind};
use super::scalar_type::ScalarType;

const HASH_CONTEXT: &str = "mint block ABI fingerprint v2";

pub(crate) fn fingerprint(resolved: &ResolvedLayout<'_>) -> Result<u64, LayoutError> {
    let mut hasher = blake3::Hasher::new_derive_key(HASH_CONTEXT);
    let abi = resolved.abi();
    hasher.update(&[match abi.endianness() {
        Endianness::Little => 0,
        Endianness::Big => 1,
    }]);
    hash_usize(abi.address_unit_bits(), &mut hasher)?;
    hash_node(&resolved.root, resolved, &mut hasher)?;

    let digest = hasher.finalize();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest.as_bytes()[..8]);
    Ok(u64::from_be_bytes(bytes))
}

fn hash_node(
    node: &ResolvedNode<'_>,
    resolved: &ResolvedLayout<'_>,
    hasher: &mut blake3::Hasher,
) -> Result<(), LayoutError> {
    match node {
        ResolvedNode::Branch {
            coordinates,
            children,
        } => {
            hasher.update(&[0]);
            hash_usize(coordinates.offset, hasher)?;
            hash_usize(coordinates.size, hasher)?;
            hash_usize(coordinates.alignment, hasher)?;
            hash_usize(children.len(), hasher)?;
            for (_, child) in children {
                hash_node(child, resolved, hasher)?;
            }
        }
        ResolvedNode::Leaf {
            coordinates,
            scalar_abi,
            leaf,
        } => {
            hasher.update(&[1]);
            hash_usize(coordinates.offset, hasher)?;
            hash_usize(coordinates.size, hasher)?;
            hash_usize(coordinates.alignment, hasher)?;
            hash_usize(scalar_abi.storage_size, hasher)?;
            hash_usize(scalar_abi.alignment, hasher)?;
            hash_usize(scalar_abi.array_stride, hasher)?;
            hash_scalar(leaf.scalar_type, hasher);
            hash_dimensions(leaf.size.as_ref(), hasher)?;
            match &leaf.source {
                EntrySource::Bitmap(fields) => {
                    hasher.update(&[1]);
                    hash_usize(fields.len(), hasher)?;
                    for field in fields {
                        hash_usize(field.bits, hasher)?;
                    }
                }
                EntrySource::Ref(RefSource::Scalar(RefTarget::Path(path))) => {
                    hasher.update(&[2]);
                    hash_resolved_ref_target(path, resolved, hasher)?;
                }
                EntrySource::Ref(RefSource::Scalar(RefTarget::Address(_))) => {
                    hasher.update(&[2, 2]);
                }
                EntrySource::Ref(RefSource::List(targets)) => {
                    hasher.update(&[2, 3]);
                    let Some(SizeSource::OneD(capacity)) = leaf.size else {
                        return Err(layout_size_error(
                            "ref list dimensions disappeared after resolution",
                        ));
                    };
                    for index in 0..capacity {
                        match targets.get(index) {
                            Some(RefTarget::Path(path)) => {
                                hash_resolved_ref_target(path, resolved, hasher)?;
                            }
                            Some(RefTarget::Address(_)) | None => {
                                hasher.update(&[2]);
                            }
                        }
                    }
                }
                _ => {
                    hasher.update(&[0]);
                }
            }
        }
    }
    Ok(())
}

fn hash_resolved_ref_target(
    path: &str,
    resolved: &ResolvedLayout<'_>,
    hasher: &mut blake3::Hasher,
) -> Result<(), LayoutError> {
    let target = resolved.target(path).ok_or_else(|| {
        layout_size_error(format!("ref target '{path}' disappeared after resolution"))
    })?;
    hasher.update(&[match target.kind {
        TargetKind::Branch => 0,
        TargetKind::Leaf => 1,
    }]);
    let target_offset = resolved
        .abi()
        .offset_to_address_units(target.coordinates.offset)?;
    hash_u64(target_offset, hasher);
    hash_usize(target.coordinates.size, hasher)?;
    hash_usize(target.coordinates.alignment, hasher)?;
    Ok(())
}

fn hash_scalar(scalar: ScalarType, hasher: &mut blake3::Hasher) {
    match scalar {
        ScalarType::U8 => {
            hasher.update(&[0]);
        }
        ScalarType::U16 => {
            hasher.update(&[1]);
        }
        ScalarType::U32 => {
            hasher.update(&[2]);
        }
        ScalarType::U64 => {
            hasher.update(&[3]);
        }
        ScalarType::I8 => {
            hasher.update(&[4]);
        }
        ScalarType::I16 => {
            hasher.update(&[5]);
        }
        ScalarType::I32 => {
            hasher.update(&[6]);
        }
        ScalarType::I64 => {
            hasher.update(&[7]);
        }
        ScalarType::F32 => {
            hasher.update(&[8]);
        }
        ScalarType::F64 => {
            hasher.update(&[9]);
        }
        ScalarType::Fixed(fixed) => {
            hasher.update(&[
                10,
                u8::from(fixed.signed),
                fixed.integer_bits,
                fixed.fractional_bits,
                fixed.total_bits(),
            ]);
        }
    };
}

fn hash_dimensions(
    dimensions: Option<&SizeSource>,
    hasher: &mut blake3::Hasher,
) -> Result<(), LayoutError> {
    match dimensions {
        None => {
            hasher.update(&[0]);
        }
        Some(SizeSource::OneD(length)) => {
            hasher.update(&[1]);
            hash_usize(*length, hasher)?;
        }
        Some(SizeSource::TwoD([rows, columns])) => {
            hasher.update(&[2]);
            hash_usize(*rows, hasher)?;
            hash_usize(*columns, hasher)?;
        }
    }
    Ok(())
}

fn hash_usize(value: usize, hasher: &mut blake3::Hasher) -> Result<(), LayoutError> {
    let value = u64::try_from(value).map_err(|_| layout_size_error("layout value exceeds u64"))?;
    hash_u64(value, hasher);
    Ok(())
}

fn hash_u64(value: u64, hasher: &mut blake3::Hasher) {
    hasher.update(&value.to_le_bytes());
}

fn layout_size_error(message: impl Into<String>) -> LayoutError {
    LayoutError::InvalidLayout(message.into())
}
