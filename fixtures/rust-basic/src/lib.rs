use std::collections::HashMap;

pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub fn subtract(a: i32, b: i32) -> i32 {
    a - b
}

pub struct Point {
    pub x: f64,
    pub y: f64,
}

pub struct Rectangle {
    pub origin: Point,
    pub width: f64,
    pub height: f64,
}

pub enum Direction {
    North,
    South,
    East,
    West,
}

pub trait Shape {
    fn area(&self) -> f64;
    fn perimeter(&self) -> f64;
}

impl Shape for Rectangle {
    fn area(&self) -> f64 {
        self.width * self.height
    }

    fn perimeter(&self) -> f64 {
        2.0 * (self.width + self.height)
    }
}

impl Rectangle {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Rectangle {
            origin: Point { x, y },
            width,
            height,
        }
    }
}

pub const MAX_SIZE: usize = 1024;

pub type NameMap = HashMap<String, String>;
