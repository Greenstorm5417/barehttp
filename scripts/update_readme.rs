use std::fs;
use std::io;

fn main() -> io::Result<()> {
    let lib_content = fs::read_to_string("src/lib.rs")?;
    
    let mut doc_lines = Vec::new();
    for line in lib_content.lines() {
        if let Some(doc_content) = line.trim_start().strip_prefix("//!") {
            let content = doc_content.strip_prefix(' ').unwrap_or(doc_content);
            doc_lines.push(content);
        } else if !line.trim().is_empty() && !line.trim().starts_with("//!") {
            if !doc_lines.is_empty() {
                break;
            }
        }
    }
    
    let mut readme = String::new();
    for line in doc_lines {
        readme.push_str(line);
        readme.push('\n');
    }
    
    readme.push_str("\n## License\n\n");
    readme.push_str("Licensed under either of:\n\n");
    readme.push_str("- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)\n");
    readme.push_str("- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)\n\n");
    readme.push_str("at your option.\n\n");
    readme.push_str("## Contribution\n\n");
    readme.push_str("Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.\n");
    
    fs::write("README.md", readme)?;
    println!("✓ README.md updated successfully");
    
    Ok(())
}
