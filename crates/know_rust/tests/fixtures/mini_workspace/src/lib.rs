//! Mini workspace fixture for know_rust integration tests.
//!
//! Exercises a small but representative set of constructs: struct +
//! enum + trait + impl + derive + free fn + module + use statement +
//! generic bound + field types.

use std::collections::HashMap;

pub struct Widget {
    pub name: String,
    pub count: usize,
}

pub trait Renderer {
    fn render(&self) -> String;
    fn name(&self) -> &str {
        "default"
    }
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
}

impl Renderer for Widget {
    fn render(&self) -> String {
        format!("{}:{}", self.name, self.count)
    }
}

pub fn build_widget(name: String, count: usize) -> Widget {
    Widget { name, count }
}

pub fn count_widgets(widgets: &[Widget]) -> HashMap<String, usize> {
    let mut map = HashMap::new();
    for w in widgets {
        *map.entry(w.name.clone()).or_default() += 1;
    }
    map
}

pub enum Color {
    Red,
    Green,
    Blue,
}

pub mod sub {
    pub fn helper(s: &str) -> String {
        s.to_uppercase()
    }
}
