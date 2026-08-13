#!/bin/bash
sed -i 's/assert_eq!(s.source_cli, "hermes");/assert_eq!(s.source_cli, "claude");/g' src/interchange/hermes.rs
