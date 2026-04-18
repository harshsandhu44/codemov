use std::fmt;

pub struct Greeter {
    name: String,
}

impl Greeter {
    pub fn new(name: &str) -> Self {
        Greeter {
            name: name.to_string(),
        }
    }

    pub fn greet(&self) -> String {
        format!("Hello, {}!", self.name)
    }
}

pub fn run() -> fmt::Result {
    let g = Greeter::new("world");
    println!("{}", g.greet());
    Ok(())
}
