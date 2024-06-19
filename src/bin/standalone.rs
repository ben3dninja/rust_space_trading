use std::{env, error::Error};

use rust_space_trading::{app::body_data::BodyType, keyboard::Keymap, standalone::Standalone};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args();
    let mut keymap = Keymap::default();
    if let Some(command) = args.nth(1) {
        match &command[..] {
            "--writekeymap" => {
                Keymap::default()
                    .write_to_file(args.next().ok_or("Expected output file path")?, false)?;
            }
            "-k" => {
                keymap = Keymap::from_toml_file(args.next().ok_or("Expected keymap file path")?)?;
            }
            _ => {}
        }
    }
    let body_type = BodyType::Moon;
    #[cfg(feature = "asteroids")]
    let body_type = BodyType::Asteroid;
    Standalone::new(body_type)?.with_keymap(keymap).run()
}
