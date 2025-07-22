use anyhow::Result;
use colored::Colorize;
use saphyr_parser::{Event, Parser, Span, SpannedEventReceiver};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Override {
    pub file: String,
    pub path: Vec<String>,
    pub value: String,
    pub line: usize,
    pub previous_value: String,
    pub previous_file: String,
    pub previous_line: usize,
}

impl fmt::Display for Override {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "  {} {}:{}", "File:".bold(), self.file, self.line)?;
        writeln!(f, "  {} {}", "Path:".bold(), self.path.join("."))?;
        writeln!(f, "  {} {}", "Value:".bold(), self.value)?;
        writeln!(
            f,
            "  {} {} (from {}:{})",
            "Same as:".bold(),
            self.previous_value,
            self.previous_file,
            self.previous_line
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct DuplicateKeyWarning {
    pub file: String,
    pub path: Vec<String>,
    pub first_value: String,
    pub first_line: usize,
    pub second_value: String,
    pub second_line: usize,
}

impl fmt::Display for DuplicateKeyWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "  {} {}", "File:".bold(), self.file)?;
        writeln!(f, "  {} {}", "Path:".bold(), self.path.join("."))?;
        writeln!(
            f,
            "  {} {} (line {})",
            "First value:".bold(),
            self.first_value,
            self.first_line
        )?;
        writeln!(
            f,
            "  {} {} (line {})",
            "Second value:".bold(),
            self.second_value,
            self.second_line
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct ValueWithLocation {
    value: String,
    file: String,
    line: usize,
}

#[derive(Debug)]
enum ParseState {
    Idle,
    ExpectingKey,
    ExpectingValue(String), // The key
    InSequence,
}

struct YamlValueCollector {
    values: Vec<(Vec<String>, ValueWithLocation)>, // Using Vec to preserve order and handle duplicates
    current_path: Vec<String>,
    current_file: String,
    state: ParseState,
    sequence_index: usize,
    mapping_depth: usize,
    current_sequence_items: Vec<String>, // Collect items in current sequence
    sequence_start_line: usize,
    sequence_depth: usize, // Track how deeply nested we are in sequences
}

impl YamlValueCollector {
    fn new(file: String) -> Self {
        Self {
            values: Vec::new(),
            current_path: Vec::new(),
            current_file: file,
            state: ParseState::Idle,
            sequence_index: 0,
            mapping_depth: 0,
            current_sequence_items: Vec::new(),
            sequence_start_line: 0,
            sequence_depth: 0,
        }
    }
}

impl<'input> SpannedEventReceiver<'input> for YamlValueCollector {
    fn on_event(&mut self, event: Event<'input>, span: Span) {
        match event {
            Event::MappingStart(_, _) => {
                if let ParseState::ExpectingValue(key) = &self.state {
                    // This is a nested mapping as a value
                    self.current_path.push(key.clone());
                }
                self.mapping_depth += 1;
                // If we're in a sequence, stay in the InSequence state
                if self.sequence_depth == 0 {
                    self.state = ParseState::ExpectingKey;
                }
            }
            Event::MappingEnd => {
                self.mapping_depth -= 1;
                if !self.current_path.is_empty()
                    && self.current_path.len() >= self.mapping_depth
                    && self.sequence_depth == 0
                {
                    self.current_path.pop();
                }
                // If we're not in a sequence, update the state
                if self.sequence_depth == 0 {
                    self.state = if self.mapping_depth > 0 {
                        ParseState::ExpectingKey
                    } else {
                        ParseState::Idle
                    };
                }
            }
            Event::SequenceStart(_, _) => {
                self.sequence_depth += 1;
                if let ParseState::ExpectingValue(key) = &self.state {
                    // This is a sequence as a value - start collecting sequence items
                    self.current_path.push(key.clone());
                    self.current_sequence_items.clear();
                    self.sequence_start_line = span.start.line();
                }
                self.state = ParseState::InSequence;
                self.sequence_index = 0;
            }
            Event::SequenceEnd => {
                self.sequence_depth -= 1;
                // End of sequence - record the entire sequence as one value
                if !self.current_path.is_empty() && self.sequence_depth == 0 {
                    let sequence_value = format!("[{}]", self.current_sequence_items.join(", "));
                    self.values.push((
                        self.current_path.clone(),
                        ValueWithLocation {
                            value: sequence_value,
                            file: self.current_file.clone(),
                            line: self.sequence_start_line,
                        },
                    ));
                    self.current_path.pop();
                }
                self.current_sequence_items.clear();
                self.state = if self.mapping_depth > 0 {
                    ParseState::ExpectingKey
                } else {
                    ParseState::Idle
                };
            }
            Event::Scalar(value, _, _, _) => {
                match &self.state {
                    ParseState::ExpectingKey => {
                        // This is a key
                        self.state = ParseState::ExpectingValue(value.into_owned());
                    }
                    ParseState::ExpectingValue(key) => {
                        // This is a scalar value for the key
                        // Only collect values if we're not inside a sequence
                        if self.sequence_depth == 0 {
                            let mut value_path = self.current_path.clone();
                            value_path.push(key.clone());

                            let line = span.start.line();
                            self.values.push((
                                value_path,
                                ValueWithLocation {
                                    value: value.into_owned(),
                                    file: self.current_file.clone(),
                                    line,
                                },
                            ));
                        }

                        self.state = ParseState::ExpectingKey;
                    }
                    ParseState::InSequence => {
                        // This is an item in a sequence - collect it
                        self.current_sequence_items.push(format!("\"{value}\""));
                        self.sequence_index += 1;
                    }
                    ParseState::Idle => {
                        // Root level scalar
                        let line = span.start.line();
                        self.values.push((
                            vec![],
                            ValueWithLocation {
                                value: value.into_owned(),
                                file: self.current_file.clone(),
                                line,
                            },
                        ));
                    }
                }
            }
            _ => {}
        }
    }
}

pub struct PointlessPointer {
    base_file: PathBuf,
    override_files: Vec<PathBuf>,
}

impl PointlessPointer {
    pub fn new(base_file: PathBuf, override_files: Vec<PathBuf>) -> Self {
        Self {
            base_file,
            override_files,
        }
    }

    pub fn analyze(&self) -> Result<(Vec<Override>, Vec<DuplicateKeyWarning>)> {
        // Collect all values from all files
        let mut all_values: Vec<Vec<(Vec<String>, ValueWithLocation)>> = Vec::new();

        // Process base file
        let base_content = fs::read_to_string(&self.base_file)?;
        let mut base_collector = YamlValueCollector::new(self.base_file.display().to_string());
        let mut parser = Parser::new_from_str(&base_content);
        parser.load(&mut base_collector, true)?;
        all_values.push(base_collector.values);

        // Process override files
        for override_file in &self.override_files {
            let content = fs::read_to_string(override_file)?;
            let mut collector = YamlValueCollector::new(override_file.display().to_string());
            let mut parser = Parser::new_from_str(&content);
            parser.load(&mut collector, true)?;
            all_values.push(collector.values);
        }

        Ok(find_pointless_overrides_and_warnings(&all_values))
    }
}

fn find_pointless_overrides_and_warnings(
    all_values: &[Vec<(Vec<String>, ValueWithLocation)>],
) -> (Vec<Override>, Vec<DuplicateKeyWarning>) {
    let mut pointless = Vec::new();
    let mut warnings = Vec::new();

    // Check for duplicates within each file first
    for values in all_values.iter() {
        let mut seen_in_file: HashMap<Vec<String>, &ValueWithLocation> = HashMap::new();

        for (path, value_loc) in values {
            if let Some(previous_in_file) = seen_in_file.get(path) {
                // Found a duplicate within the same file
                if value_loc.value == previous_in_file.value {
                    pointless.push(Override {
                        file: value_loc.file.clone(),
                        path: path.clone(),
                        value: value_loc.value.clone(),
                        line: value_loc.line,
                        previous_value: previous_in_file.value.clone(),
                        previous_file: previous_in_file.file.clone(),
                        previous_line: previous_in_file.line,
                    });
                } else {
                    // Same key but different values - create a warning
                    warnings.push(DuplicateKeyWarning {
                        file: value_loc.file.clone(),
                        path: path.clone(),
                        first_value: previous_in_file.value.clone(),
                        first_line: previous_in_file.line,
                        second_value: value_loc.value.clone(),
                        second_line: value_loc.line,
                    });
                }
            }
            seen_in_file.insert(path.clone(), value_loc);
        }
    }

    // Then check for overrides across files
    if all_values.len() >= 2 {
        // For each override file (starting from the second)
        for i in 1..all_values.len() {
            let current_values = &all_values[i];

            // Build effective values up to the previous file
            // Using HashMap to get the last value for each path (in case of duplicates)
            let mut effective_values: HashMap<Vec<String>, ValueWithLocation> = HashMap::new();
            for value in all_values.iter().take(i) {
                for (path, value_loc) in value {
                    effective_values.insert(path.clone(), value_loc.clone());
                }
            }

            // Check current file for pointless overrides
            for (path, current_value) in current_values {
                if let Some(previous_value) = effective_values.get(path) {
                    if current_value.value == previous_value.value {
                        pointless.push(Override {
                            file: current_value.file.clone(),
                            path: path.clone(),
                            value: current_value.value.clone(),
                            line: current_value.line,
                            previous_value: previous_value.value.clone(),
                            previous_file: previous_value.file.clone(),
                            previous_line: previous_value.line,
                        });
                    }
                }
            }
        }
    }

    (pointless, warnings)
}
