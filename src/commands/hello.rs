use clap::Args;

#[derive(Args)]
pub struct HelloArgs {
    /// Optional name to greet
    name: Option<String>,
}

impl HelloArgs {
    pub fn run(self) {
        match self.name {
            Some(name) => println!("Hello, {name}!"),
            None => println!("Hello!"),
        }
    }
}
