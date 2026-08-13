#!/bin/bash
sed -i 's/source_cli: "hermes".to_string(),/source_cli: "hermes".to_string(),\n        source_version: String::new(),/g' src/interchange/hermes.rs
