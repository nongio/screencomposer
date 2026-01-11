# Skia Renderer Partial Rendering Implementation Plan

## YES - You Can Implement True Partial Rendering in Skia Without Touching Smithay!

### Current Skia Implementation Analysis

**Good News:**
1. ✅ Skia `SkiaFrame` already receives damage rectangles in both `clear()` and `render_texture_from_to()`
2. ✅ Skia has `canvas.clip_rect()` which can restrict rendering (currently only used in `draw_solid`)
3. ✅ Skia supports `canvas.save()/restore()` for clip stack management
4. ✅ You control the entire Frame implementation - no Smithay changes needed

**Current Issues:**
1. ❌ `clear()` uses a single clip rect for ALL damage (line 744: `canvas.clip_rect(damage_rect, ...)`)
2. ❌ `render_texture_from_to()` draws full texture for each damage rect without clipping (line 836)
3. ⚠️ Multiple damage rects are joined into one bbox, wasting GPU cycles

### Implementation Strategy

You can implement **SCISSOR-EQUIVALENT** behavior using Skia's clip rect system:

#### Option 1: Per-Damage-Rect Clipping (RECOMMENDED)

**Advantage:** True partial rendering, optimal GPU usage
**Disadvantage:** Multiple save/restore pairs

```rust
// In src/skia_renderer.rs

// MODIFY: clear() - line 689
fn clear(
    &mut self,
    color: Color32F,
    at: &[Rectangle<i32, Physical>],
) -> Result<(), Self::Error> {
    if at.is_empty() {
        return Ok(());
    }

    let dest_rect = skia::Rect::from_xywh(
        0.0,
        0.0,
        self.size.w as f32,
        self.size.h as f32,
    );
    
    let color = skia::Color4f::new(color.r(), color.g(), color.b(), color.a());
    let mut paint = skia::Paint::new(color, None);
    paint.set_blend_mode(skia::BlendMode::Src);

    let mut surface = self.skia_surface.clone();
    let canvas = surface.canvas();

    // Render ONLY damaged regions using clip_rect
    for rect in at {
        let clip_rect = skia::Rect::from_xywh(
            rect.loc.x as f32,
            rect.loc.y as f32,
            rect.size.w as f32,
            rect.size.h as f32,
        );
        
        canvas.save();
        // Clip to damage rect - equivalent to glScissor
        canvas.clip_rect(clip_rect, None, None);
        canvas.draw_rect(dest_rect, &paint);
        canvas.restore();
    }

    Ok(())
}

// MODIFY: render_texture_from_to() - line 751
fn render_texture_from_to(
    &mut self,
    texture: &Self::TextureId,
    src: Rectangle<f64, Buffer>,
    dst: Rectangle<i32, Physical>,
    damage: &[Rectangle<i32, Physical>],
    _opaque_regions: &[Rectangle<i32, Physical>],
    src_transform: Transform,
    alpha: f32,
) -> Result<(), Self::Error> {
    if damage.is_empty() {
        return Ok(());
    }

    let image = &texture.image;
    let mut paint = skia::Paint::new(skia::Color4f::new(1.0, 1.0, 1.0, alpha), None);
    paint.set_blend_mode(skia::BlendMode::SrcOver);

    let mut matrix = skia::Matrix::new_identity();
    let mut surface = self.skia_surface.clone();
    let canvas = surface.canvas();
    
    // Setup transform matrix (existing code)
    let scale_x = dst.size.w as f32 / src.size.w as f32;
    let scale_y = dst.size.h as f32 / src.size.h as f32;
    match src_transform {
        Transform::Normal => {
            matrix.pre_scale((scale_x, scale_y), None);
            matrix.pre_translate((
                dst.loc.x as f32 / scale_x - (src.loc.x as f32),
                dst.loc.y as f32 / scale_y - (src.loc.y as f32),
            ));
        }
        Transform::Flipped180 => {
            matrix.pre_scale((scale_x, -scale_y), None);
            matrix.pre_translate((
                dst.loc.x as f32 / scale_x - src.loc.x as f32,
                -dst.loc.y as f32 / scale_y + src.loc.y as f32,
            ));
        }
        _ => panic!("unhandled transform {:?}", src_transform),
    }

    // Setup shader ONCE outside loop
    paint.set_shader(image.to_shader(
        (skia::TileMode::Repeat, skia::TileMode::Repeat),
        skia::SamplingOptions::default(),
        &matrix,
    ));

    // Render ONLY damaged regions
    for rect in damage {
        // Clip to damage rect in destination space
        let clip_rect = skia::Rect::from_xywh(
            (dst.loc.x + rect.loc.x) as f32,
            (dst.loc.y + rect.loc.y) as f32,
            rect.size.w as f32,
            rect.size.h as f32,
        );
        
        let draw_rect = skia::Rect::from_xywh(
            dst.loc.x as f32,
            dst.loc.y as f32,
            dst.size.w as f32,
            dst.size.h as f32,
        );

        canvas.save();
        canvas.clip_rect(clip_rect, None, None);
        canvas.draw_rect(draw_rect, &paint);
        canvas.restore();
    }

    Ok(())
}
```

#### Option 2: Optimize with Clip Region (ADVANCED)

If you have many small damage rects, use Skia's `Region` for complex clipping:

```rust
use lay_rs::skia::Region;

fn render_texture_from_to(...) -> Result<(), Self::Error> {
    if damage.is_empty() {
        return Ok(());
    }

    // Build region from all damage rects
    let mut clip_region = Region::new();
    for rect in damage {
        let irect = skia::IRect::from_xywh(
            (dst.loc.x + rect.loc.x) as i32,
            (dst.loc.y + rect.loc.y) as i32,
            rect.size.w as i32,
            rect.size.h as i32,
        );
        clip_region.op_irect(irect, skia::region::RegionOp::Union);
    }

    let canvas = surface.canvas();
    canvas.save();
    canvas.clip_region(&clip_region, None);
    
    // Draw ONCE with complex clip
    canvas.draw_rect(draw_rect, &paint);
    
    canvas.restore();
    Ok(())
}
```

### Performance Validation

Add metrics to measure effectiveness:

```rust
// Add to SkiaFrame struct
#[cfg(feature = "damage-metrics")]
struct DamageMetrics {
    total_area: u64,
    damaged_area: u64,
}

#[cfg(feature = "damage-metrics")]
impl SkiaFrame<'_> {
    fn log_damage_metrics(&self, damage: &[Rectangle<i32, Physical>]) {
        let total = (self.size.w * self.size.h) as u64;
        let damaged: u64 = damage.iter()
            .map(|r| (r.size.w * r.size.h) as u64)
            .sum();
        
        let ratio = (damaged as f32 / total as f32) * 100.0;
        tracing::debug!(
            "Damage: {:.1}% ({}/{} px, {} rects)",
            ratio, damaged, total, damage.len()
        );
    }
}

// Call before clear/render_texture_from_to
#[cfg(feature = "damage-metrics")]
self.log_damage_metrics(at);
```

### Testing Strategy

1. **Visual Verification:**
   ```rust
   // Temporarily highlight damage regions
   #[cfg(debug_assertions)]
   {
       let mut debug_paint = skia::Paint::default();
       debug_paint.set_color(skia::Color::from_argb(128, 255, 0, 0));
       debug_paint.set_style(skia::paint::Style::Stroke);
       debug_paint.set_stroke_width(2.0);
       
       for rect in damage {
           canvas.draw_rect(skia::Rect::from_xywh(...), &debug_paint);
       }
   }
   ```

2. **Performance Testing:**
   - Run with `RUST_LOG=screencomposer=debug,damage-metrics`
   - Monitor: `cargo build --release --features damage-metrics`
   - Measure frame times before/after
   - Expected: 60-80% reduction for typical workloads

3. **Correctness Testing:**
   - Move single window slowly - should see small damage
   - Fullscreen video - should see large damage
   - Typing in terminal - should see character-sized damage
   - Check for artifacts (indicates incorrect clipping)

### Integration Points

**No Smithay Changes Required!** Only modify:
1. `src/skia_renderer.rs` - Update `SkiaFrame::clear()` and `SkiaFrame::render_texture_from_to()`
2. Optional: Add metrics feature flag to `Cargo.toml`

### Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Skia clip state corruption | Always use save()/restore() pairs |
| Performance regression | Add feature flag, measure before/after |
| Artifacts from incorrect clips | Visual debug mode, extensive testing |
| Complex damage patterns | Use Region API for optimization |

### Expected Benefits

**Conservative Estimate:**
- 50-70% GPU time reduction (typical desktop: 10-20% screen damaged)
- 30-50% power savings on battery
- Smoother frame pacing for partial updates

**Aggressive Estimate (cursor-only movement):**
- 95%+ GPU time reduction
- Cursor: ~1000px damaged of 2M+ total pixels

### Implementation Checklist

- [ ] Add damage metrics feature flag to Cargo.toml
- [ ] Modify `SkiaFrame::clear()` with per-rect clipping
- [ ] Modify `SkiaFrame::render_texture_from_to()` with per-rect clipping
- [ ] Add debug visualization (optional)
- [ ] Add performance metrics logging
- [ ] Test with various workloads (window move, video, typing)
- [ ] Measure GPU time improvement
- [ ] Document findings in CHANGELOG

### Alternative: Hybrid Approach

If per-rect overhead is too high:

```rust
const MAX_CLIP_RECTS: usize = 16;

if damage.len() > MAX_CLIP_RECTS {
    // Use bounding box for complex damage
    let bbox = compute_bounding_box(damage);
    canvas.clip_rect(bbox, None, None);
    canvas.draw_rect(draw_rect, &paint);
} else {
    // Use per-rect clipping for simple damage
    for rect in damage {
        canvas.save();
        canvas.clip_rect(rect, None, None);
        canvas.draw_rect(draw_rect, &paint);
        canvas.restore();
    }
}
```

---

## CONCLUSION

**YES - You can implement true partial rendering entirely in the Skia renderer without touching Smithay.**

Skia's `canvas.clip_rect()` is functionally equivalent to OpenGL's scissor test. The damage tracking infrastructure is already in place - you just need to use the clip API correctly.

**Recommended Next Steps:**
1. Implement Option 1 (per-rect clipping) in `skia_renderer.rs`
2. Add metrics to measure improvement
3. Test thoroughly
4. If performance is good, keep it. If there's overhead from many save/restore pairs, optimize with Region API or hybrid approach.

**Estimated Implementation Time:** 2-4 hours
**Estimated Testing Time:** 2-3 hours
**Risk Level:** Low (local changes, easily reversible)
