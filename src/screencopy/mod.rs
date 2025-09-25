#![allow(dead_code)]

pub mod protocol;

#[derive(Debug, Default)]
pub struct ScreencopyManager;

impl ScreencopyManager {
    pub fn new() -> Self {
        Self::default()
    }
}
