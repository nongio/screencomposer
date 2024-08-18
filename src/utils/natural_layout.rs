use std::collections::HashMap;

use smithay::reexports::wayland_server::{backend::ObjectId, Resource};

use crate::workspace::Window;


trait LayoutBoundingBox {
    fn bounding_box(&self) -> LayoutRect;
}

impl LayoutBoundingBox for Window {
    fn bounding_box(&self) -> LayoutRect {
        // Return the bounding box of the window
        LayoutRect::new(self.x, self.y, self.w, self.h)
    }
}

const WINDOW_PLACEMENT_NATURAL_ACCURACY: f32 = 20.0;
const WINDOW_PLACEMENT_NATURAL_GAPS: f32 = 10.0;
const WINDOW_PLACEMENT_NATURAL_MAX_TRANSLATIONS: usize = 5000;

#[derive(Clone)]
pub struct LayoutRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl LayoutRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        LayoutRect { x, y, width, height }
    }

    fn copy(&self) -> Self {
        self.clone()
    }

    fn union(&self, rect2: &LayoutRect) -> Self {
        let mut dest = self.copy();
        if rect2.x < dest.x {
            dest.width += dest.x - rect2.x;
            dest.x = rect2.x;
        }
        if rect2.y < dest.y {
            dest.height += dest.y - rect2.y;
            dest.y = rect2.y;
        }
        if rect2.x + rect2.width > dest.x + dest.width {
            dest.width = rect2.x + rect2.width - dest.x;
        }
        if rect2.y + rect2.height > dest.y + dest.height {
            dest.height = rect2.y + rect2.height - dest.y;
        }
        dest
    }

    fn adjusted(&self, dx: f32, dy: f32, dx2: f32, dy2: f32) -> Self {
        let mut dest = self.copy();
        dest.x += dx;
        dest.y += dy;
        dest.width += -dx + dx2;
        dest.height += -dy + dy2;
        dest
    }

    fn overlap(&self, rect2: &LayoutRect) -> bool {
        !(self.x + self.width <= rect2.x ||
          rect2.x + rect2.width <= self.x ||
          self.y + self.height <= rect2.y ||
          rect2.y + rect2.height <= self.y)
    }

    fn center(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    fn translate(&mut self, dx: f32, dy: f32) {
        self.x += dx;
        self.y += dy;
    }
}

pub fn natural_layout(windows: &Vec<Window>, area: &LayoutRect, use_more_screen: bool) -> HashMap<ObjectId, LayoutRect> {
    let area_rect = area.copy();
    let mut bounds = area_rect.copy();

    let mut direction = 0;
    let mut directions = vec![];
    let mut rects = vec![];
    for window in windows {
        let rect = window.bounding_box();
        rects.push(LayoutRect::new(rect.x, rect.y, rect.width, rect.height));
        bounds = bounds.union(rects.last().unwrap());

        directions.push(direction);
        direction = (direction + 1) % 4;
    }

    let mut loop_counter = 0;
    let mut overlap;
    loop {
        overlap = false;
        for i in 0..rects.len() {
            for j in 0..rects.len() {
                if i != j {
                    let adjustments = [-1.0, -1.0, 1.0, 1.0].map(|v| v * WINDOW_PLACEMENT_NATURAL_GAPS);
                    let i_adjusted = rects[i].adjusted(adjustments[0], adjustments[1], adjustments[2], adjustments[3]);
                    let j_adjusted = rects[j].adjusted(adjustments[0], adjustments[1], adjustments[2], adjustments[3]);
                    if i_adjusted.overlap(&j_adjusted) {
                        loop_counter += 1;
                        overlap = true;

                        let i_center = rects[i].center();
                        let j_center = rects[j].center();
                        let mut diff = (j_center.0 - i_center.0, j_center.1 - i_center.1);

                        if diff.0 == 0.0 && diff.1 == 0.0 {
                            diff.0 = 1.0;
                        }
                        if bounds.height / bounds.width > area_rect.height / area_rect.width {
                            diff.0 *= 2.0;
                        } else {
                            diff.1 *= 2.0;
                        }

                        let length = (diff.0 * diff.0 + diff.1 * diff.1).sqrt();
                        diff.0 = diff.0 * WINDOW_PLACEMENT_NATURAL_ACCURACY / length;
                        diff.1 = diff.1 * WINDOW_PLACEMENT_NATURAL_ACCURACY / length;

                        rects[i].translate(-diff.0, -diff.1);
                        rects[j].translate(diff.0, diff.1);

                        if use_more_screen {
                            let mut x_section = ((rects[i].x - bounds.x) / (bounds.width / 3.0)).round() as i32;
                            let mut y_section = ((rects[i].y - bounds.y) / (bounds.height / 3.0)).round() as i32;

                            let mut diff = (0.0, 0.0);
                            if x_section != 1 || y_section != 1 {
                                if x_section == 1 {
                                    x_section = if directions[i] / 2 == 0 { 0 } else { 2 };
                                }
                                if y_section == 1 {
                                    y_section = if directions[i] % 2 == 0 { 0 } else { 2 };
                                }
                            }
                            if x_section == 0 && y_section == 0 {
                                diff.0 = bounds.x - rects[i].center().0;
                                diff.1 = bounds.y - rects[i].center().1;
                            }
                            if x_section == 2 && y_section == 0 {
                                diff.0 = bounds.x + bounds.width - rects[i].center().0;
                                diff.1 = bounds.y - rects[i].center().1;
                            }
                            if x_section == 2 && y_section == 2 {
                                diff.0 = bounds.x + bounds.width - rects[i].center().0;
                                diff.1 = bounds.y + bounds.height - rects[i].center().1;
                            }
                            if x_section == 0 && y_section == 2 {
                                diff.0 = bounds.x - rects[i].center().0;
                                diff.1 = bounds.y + bounds.height - rects[i].center().1;
                            }
                            if diff.0 != 0.0 || diff.1 != 0.0 {
                                let length = (diff.0 * diff.0 + diff.1 * diff.1).sqrt();
                                diff.0 *= WINDOW_PLACEMENT_NATURAL_ACCURACY / length / 2.0;
                                diff.1 *= WINDOW_PLACEMENT_NATURAL_ACCURACY / length / 2.0;
                                rects[i].translate(diff.0, diff.1);
                            }
                        }

                        bounds = bounds.union(&rects[i]);
                        bounds = bounds.union(&rects[j]);
                    }
                }
            }
        }
        if !overlap || loop_counter >= WINDOW_PLACEMENT_NATURAL_MAX_TRANSLATIONS {
            break;
        }
    }

    let scale = (area_rect.width / bounds.width)
        .min(area_rect.height / bounds.height)
        .min(1.0);

    bounds.x -= (area_rect.width - bounds.width * scale) / 2.0;
    bounds.y -= (area_rect.height - bounds.height * scale) / 2.0;
    bounds.width = area_rect.width / scale;
    bounds.height = area_rect.height / scale;

    for rect in &mut rects {
        rect.translate(-bounds.x, -bounds.y);
    }

    let mut slots = HashMap::new();

    for (i, rect) in rects.iter_mut().enumerate() {
        let win = windows[i].clone();
        rect.x = rect.x * scale + area_rect.x;
        rect.y = rect.y * scale + area_rect.y;
        rect.width *= scale;
        rect.height *= scale;
        slots.insert(win.wl_surface.unwrap().id(), rect.clone());
        // slots.push((rect.x, rect.y, rect.width, rect.height, windows[i].clone()));
    }

    slots
}