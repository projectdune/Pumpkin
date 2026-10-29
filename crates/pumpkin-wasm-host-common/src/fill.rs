use pumpkin_core::world::World;
use pumpkin_data::BlockStateId;
use pumpkin_util::math::pool::Weighted;
use pumpkin_util::math::position::BlockPos;

/// Validates a plugin's `fill-cuboid` palette of `(state, weight)` pairs.
pub fn fill_palette(
    entries: impl IntoIterator<Item = (u16, u32)>,
) -> Result<Vec<Weighted<BlockStateId>>, String> {
    let mut palette = Vec::new();
    let mut total_weight: u64 = 0;
    for (state, weight) in entries {
        let Some(state_id) = BlockStateId::new(state) else {
            return Err(format!("invalid block state id {state}"));
        };
        total_weight += u64::from(weight);
        let Ok(weight) = i32::try_from(weight) else {
            return Err("palette weights must add up to at most 2147483647".to_string());
        };
        palette.push(Weighted {
            data: state_id,
            weight,
        });
    }
    if total_weight == 0 {
        return Err("palette has no weight".to_string());
    }
    if total_weight > i32::MAX as u64 {
        return Err("palette weights must add up to at most 2147483647".to_string());
    }
    Ok(palette)
}

/// Number of blocks in the cuboid between two corners, both inclusive.
#[must_use]
pub fn fill_volume(min: BlockPos, max: BlockPos) -> u64 {
    (u64::from(min.0.x.abs_diff(max.0.x)) + 1)
        * (u64::from(min.0.y.abs_diff(max.0.y)) + 1)
        * (u64::from(min.0.z.abs_diff(max.0.z)) + 1)
}

/// `check_fill_volume` for a box given by its minimum corner and size (`paste-states`).
pub fn check_box_volume(world: &World, origin: BlockPos, size: (u32, u32, u32)) -> Result<(), String> {
    if size.0 == 0 || size.1 == 0 || size.2 == 0 {
        return Ok(());
    }
    let max = BlockPos::new(
        origin.0.x + size.0 as i32 - 1,
        origin.0.y + size.1 as i32 - 1,
        origin.0.z + size.2 as i32 - 1,
    );
    check_fill_volume(world, origin, max)
}

/// Rejects fills larger than the world's `max_block_modifications` game rule, as `/fill` does.
pub fn check_fill_volume(world: &World, min: BlockPos, max: BlockPos) -> Result<(), String> {
    let volume = fill_volume(min, max);
    let limit = world.level_info.load().game_rules.max_block_modifications;
    if i64::try_from(volume).unwrap_or(i64::MAX) > limit {
        return Err(format!(
            "fill volume {volume} is larger than max_block_modifications ({limit})"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_counts_both_corners_in_any_order() {
        let a = BlockPos::new(-16, 20, -16);
        let b = BlockPos::new(31, 43, 31);
        assert_eq!(fill_volume(a, b), 48 * 24 * 48);
        assert_eq!(fill_volume(b, a), 48 * 24 * 48);
        assert_eq!(fill_volume(a, a), 1);
    }

    #[test]
    fn palette_rejects_empty_and_overflowing_weights() {
        assert!(fill_palette([]).is_err());
        assert!(fill_palette([(1, 0)]).is_err());
        assert!(fill_palette([(1, i32::MAX as u32), (2, 1)]).is_err());
        assert_eq!(fill_palette([(1, 70), (2, 30)]).map(|p| p.len()), Ok(2));
    }
}
