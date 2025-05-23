use indicatif::{ProgressBar, ProgressStyle};
use serde_json;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::thread::sleep;
use std::time::Duration;

mod dictionary;
mod utils;

use dictionary::{FIRST_DICT, SECOND_DICT};
use utils::matches_pattern;

use colored::Colorize;
use dialoguer::{Input, Password, Select};

mod cli;
mod starknet_client;

use clap::{Parser, Subcommand};
use colored::*;

const APP_NAME: &str = "StarkSqueeze CLI";
const APP_ABOUT: &str = "Interact with StarkSqueeze";

pub fn file_to_binary(file_path: &str) -> io::Result<Vec<u8>> {
    let mut file = File::open(file_path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(buffer)
}

pub fn binary_to_file(
    input: &(impl AsRef<str> + ?Sized),
    output_path: Option<&str>,
) -> io::Result<()> {
    let binary_string: String = input.as_ref().split_whitespace().collect();
    if !binary_string.chars().all(|c| c == '0' || c == '1') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid binary string",
        ));
    }

    let file_path = output_path.unwrap_or("output.bin");
    let file = File::create(file_path)?;
    let mut writer = BufWriter::new(file);

    let original_length = binary_string.len() as u16;
    writer.write_all(&original_length.to_be_bytes())?;

    let padded_binary_string = pad_binary_string(&binary_string);
    let total_chunks = padded_binary_string.len() / 8;

    println!(
        "🚀 Converting binary string of size {} bits to file...",
        binary_string.len()
    );

    let pb = ProgressBar::new(total_chunks as u64);
    pb.set_style(
        ProgressStyle::with_template("🔸 [{bar:40.green/blue}] {percent}% ⏳ {msg}")
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏ "),
    );

    let mut byte_buffer = Vec::with_capacity(total_chunks);

    for (i, chunk) in padded_binary_string.as_bytes().chunks(8).enumerate() {
        let chunk_str = std::str::from_utf8(chunk).expect("Invalid UTF-8 sequence in binary data");
        let byte = u8::from_str_radix(chunk_str, 2)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid binary chunk"))?;
        byte_buffer.push(byte);

        pb.inc(1);
        pb.set_message(format!("Processing chunk {}/{}", i + 1, total_chunks));
    }

    pb.set_message("Writing bytes to file...");
    writer.write_all(&byte_buffer)?;
    writer.flush()?;

    pb.finish_with_message(format!("✅ File saved successfully to: {} 🎉", file_path));
    Ok(())
}

fn pad_binary_string(binary_string: &str) -> String {
    let padding_needed = (8 - (binary_string.len() % 8)) % 8;
    format!("{}{}", binary_string, "0".repeat(padding_needed))
}

fn unpad_binary_string(padded: &str, original_length: usize) -> String {
    padded.chars().take(original_length).collect()
}

fn read_binary_file(file_path: &str) -> io::Result<String> {
    let mut file = File::open(file_path)?;
    let mut length_bytes = [0u8; 2];
    file.read_exact(&mut length_bytes)?;
    let original_length = u16::from_be_bytes(length_bytes) as usize;
    let mut binary_string = String::new();
    let mut byte_buffer = [0u8; 1];

    while binary_string.len() < original_length {
        file.read_exact(&mut byte_buffer)?;
        let byte_binary = format!("{:08b}", byte_buffer[0]);
        binary_string.push_str(&byte_binary);
    }

    // Use unpad_binary_string to truncate to the original length
    Ok(unpad_binary_string(&binary_string, original_length))
}
pub fn split_by_5(binary_string: &str) -> String {
    if binary_string.is_empty() {
        return serde_json::json!([]).to_string();
    }

    if !binary_string.chars().all(|c| c == '0' || c == '1') {
        return serde_json::json!([]).to_string();
    }

    let total_size = binary_string.len();
    println!("🚀 Splitting binary string of size {} bits...", total_size);

    let pb = ProgressBar::new(total_size as u64);
    pb.set_style(
        ProgressStyle::with_template("🔹 [{bar:40.green/blue}] {percent}% ⏳ {msg}")
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏ "),
    );

    let chunks: Vec<String> = binary_string
        .as_bytes()
        .chunks(5)
        .enumerate()
        .map(|(i, chunk)| {
            pb.inc(chunk.len() as u64);
            pb.set_message(format!(
                "Processing chunk {}/{}",
                i + 1,
                (total_size + 4) / 5
            ));
            String::from_utf8_lossy(chunk).to_string()
        })
        .collect();

    pb.finish_with_message("✅ Splitting Complete! 🎉");
    serde_json::json!(chunks).to_string()
}

pub fn join_by_5(input: &[u8], output_path: &str) -> io::Result<()> {
    let total_size = input.len();
    println!("🚀 Processing {} bytes...", total_size);

    let file = File::create(output_path)?;
    let mut writer = BufWriter::new(file);

    let pb = ProgressBar::new(total_size as u64);
    pb.set_style(
        ProgressStyle::with_template("🔵 [{bar:40.cyan/blue}] {percent}% 🚀 {msg}")
            .unwrap()
            .progress_chars("#>-"),
    );

    for (i, chunk) in input.chunks(5).enumerate() {
        writer.write_all(chunk)?;
        pb.inc(chunk.len() as u64);
        pb.set_message(format!("Writing chunk {}/{}", i + 1, (total_size + 4) / 5));

        // Adaptive delay for smoother progress bar experience
        if total_size < 500 {
            sleep(Duration::from_millis(50));
        }
    }

    writer.flush()?;
    pb.finish_with_message("✅ Processing Complete! 🎉");
    println!("📁 File saved: {}", output_path);
    Ok(())
}

pub fn decoding_one(dot_string: &str) -> Result<String, io::Error> {
    // Handle empty input
    if dot_string.is_empty() {
        return Ok(String::new());
    }

    // Reverse the FIRST_DICT for lookup: dot_string -> 5-bit binary
    let mut reverse_dict: HashMap<&str, &str> = HashMap::new();
    for (bin, dot) in FIRST_DICT.entries() {
        if !dot.is_empty() {
            reverse_dict.insert(*dot, *bin);
        }
    }

    // Parse the dot_string into tokens
    let tokens: Vec<&str> = dot_string.split('.').filter(|t| !t.is_empty()).collect();

    let mut reconstructed_binary = String::new();

    for token in tokens {
        match reverse_dict.get(token) {
            Some(binary_chunk) => reconstructed_binary.push_str(binary_chunk),
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Unknown token '{}' found during decoding", token),
                ))
            }
        }
    }

    Ok(reconstructed_binary)
}

pub fn encoding_one(binary_string: &str) -> io::Result<String> {
    // Handle empty string case
    if binary_string.is_empty() {
        return Ok(String::new());
    }

    // Validate input - ensure only 0s and 1s
    if !binary_string.chars().all(|c| c == '0' || c == '1') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid binary string: must contain only 0 and 1",
        ));
    }

    // Pad the binary string so it's divisible by 5
    let padded_binary_string = pad_binary_string(binary_string);

    // Process the padded string in 5-bit chunks
    let mut chunks = Vec::new();
    for i in (0..padded_binary_string.len()).step_by(5) {
        if i + 5 <= padded_binary_string.len() {
            chunks.push(&padded_binary_string[i..i + 5]);
        }
    }

    // Map each chunk to its dot string representation
    let mut result = Vec::with_capacity(chunks.len());

    for chunk in &chunks {
        match FIRST_DICT.get(*chunk) {
            Some(dot_string) => {
                // Only add non-empty dot strings to the result
                if !dot_string.is_empty() {
                    result.push(*dot_string);
                }
            }
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Chunk {} not found in dictionary", chunk),
                ))
            }
        }
    }

    // Concatenate the dot strings (no separator needed)
    Ok(result.concat())
}

pub fn decoding_two(encoded_string: &str) -> Result<String, io::Error> {
    if encoded_string.is_empty() {
        return Ok(String::new());
    }

    let mut result = String::new();
    let mut chars = encoded_string.chars().peekable();

    // Iterate over the encoded string
    while chars.peek().is_some() {
        if *chars.peek().unwrap() == ' ' {
            // Skip spaces
            chars.next();
            continue;
        }

        let mut matched = false;

        // Check all possible patterns in SECOND_DICT to match the encoded part
        for length in (1..=5).rev() { // From "....." to "."
            let pattern: String = chars.clone().take(length).collect();
            if let Some(&symbol) = SECOND_DICT.get(pattern.as_str()) {
                result.push(symbol);
                // Skip the number of characters that matched
                for _ in 0..length {
                    chars.next();
                }
                matched = true;
                break;
            }
        }

        if !matched {
            let mut problematic_part = String::new();
            let mut chars_clone = chars.clone();
            for _ in 0..10 {
                if let Some(c) = chars_clone.next() {
                    problematic_part.push(c);
                } else {
                    break;
                }
            }

            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "Invalid or unknown symbol in the encoded string at position: '{}'",
                    problematic_part
                ),
            ));
        }
    }

    Ok(result)
}

pub fn encoding_two(dot_string: &str) -> Result<String, io::Error> {
    if dot_string.is_empty() {
        return Ok(String::new());
    }

    let mut result = String::new();
    let mut chars = dot_string.chars().peekable();

    while chars.peek().is_some() {
        if *chars.peek().unwrap() == ' ' {
            chars.next();
            continue;
        }

        let mut matched = false;

        let candidates = [".....", "....", "...", "..", ". .", "."];

        for &pattern in &candidates {
            if matches_pattern(&mut chars.clone(), pattern) {
                if let Some(&symbol) = SECOND_DICT.get(pattern) {
                    result.push(symbol);

                    for _ in 0..pattern.chars().count() {
                        chars.next();
                    }

                    matched = true;
                    break;
                }
            }
        }

        if !matched {
            let mut problematic_part = String::new();
            let mut chars_clone = chars.clone();
            for _ in 0..10 {
                if let Some(c) = chars_clone.next() {
                    problematic_part.push(c);
                } else {
                    break;
                }
            }

            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "Invalid or unknown dot pattern at position: '{}'",
                    problematic_part
                ),
            ));
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encoding_one() {
        // Test with "00010" (.) + "00010" (.)
        let binary = "0001000010";
        let result = encoding_one(binary).unwrap();
        assert_eq!(result, "..");

        // Test with "00111" (...) + "01110" (...)
        let binary2 = "0011101110";
        let result2 = encoding_one(binary2).unwrap();
        assert_eq!(result2, "......"); // "..." + "..."

        // Test with a longer binary string
        // "10101" (". . .") + "11111" (".....")
        let binary3 = "1010111111";
        let result3 = encoding_one(binary3).unwrap();
        assert_eq!(result3, ". . ......");

        // Test with empty string
        let result4 = encoding_one("").unwrap();
        assert_eq!(result4, "");

        // Test with invalid characters
        let invalid_binary = "001201";
        let result5 = encoding_one(invalid_binary);
        assert!(result5.is_err());

        // Test with binary that maps to empty string in FIRST_DICT
        let binary6 = "00000";
        let result6 = encoding_one(binary6).unwrap();
        assert_eq!(result6, "");

        // Test with a mix of emptry string mappings and non-empty
        // "00000" ("") + "00001" (".")
        let binary7 = "0000000001";
        let result7 = encoding_one(binary7).unwrap();
        assert_eq!(result7, ".");
    }

    #[test]
    fn test_encoding_two() {
        // Test with various patterns
        assert_eq!(encoding_two(".").unwrap(), "*");
        assert_eq!(encoding_two("..").unwrap(), "%");
        assert_eq!(encoding_two("...").unwrap(), "$");
        assert_eq!(encoding_two("....").unwrap(), "#");
        assert_eq!(encoding_two(".....").unwrap(), "!");
        assert_eq!(encoding_two(". .").unwrap(), "&");

        // Test with combinations
        assert_eq!(encoding_two(".. ...").unwrap(), "%$");
        assert_eq!(encoding_two(". . .").unwrap(), "&*");
        assert_eq!(encoding_two("...........").unwrap(), "!!*");

        // Tests with explicit spaces
        assert_eq!(encoding_two("... ...").unwrap(), "$$");
        assert_eq!(encoding_two(". . . . .").unwrap(), "&&*");
        assert_eq!(encoding_two(".. .. ..").unwrap(), "%%%");

        // Test with a mix of patterns and spaces
        let mixed = "...... . .....";
        assert_eq!(encoding_two(mixed).unwrap(), "!&!");

        // Test with leading and trailing spaces
        assert_eq!(encoding_two(" .").unwrap(), "*");
        assert_eq!(encoding_two(". ").unwrap(), "*");
        assert_eq!(encoding_two(" . ").unwrap(), "*");

        // Test with multiple consecutive spaces
        assert_eq!(encoding_two(".  .").unwrap(), "**");
        assert_eq!(encoding_two(".   .").unwrap(), "**");

        // Test with empty string
        assert_eq!(encoding_two("").unwrap(), "");

        // Test error case with invalid pattern
        assert!(encoding_two("...x").is_err());
        assert!(encoding_two("abc").is_err());
    }
}

/// CLI arguments for StarkSqueeze
#[derive(Parser, Debug)]
#[command(name = APP_NAME, about = APP_ABOUT)]
struct CliArgs {
    #[command(subcommand)]
    command: Option<Commands>,
}

/// Commands for the StarkSqueeze CLI
#[derive(Subcommand, Debug)]
enum Commands {
    /// Upload data to StarkNet
    Upload,
    /// Retrieve data from StarkNet
    Retrieve,
    /// List all uploaded data
    List,
}

#[tokio::main]
async fn main() {
    let args = CliArgs::parse();

    match args.command {
        Some(Commands::Upload) => cli::upload_data_cli().await,
        Some(Commands::Retrieve) => cli::retrieve_data_cli().await,
        Some(Commands::List) => cli::list_all_uploads().await,
        None => cli::main_menu().await,
    }
}
