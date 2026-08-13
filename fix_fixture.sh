#!/bin/bash
sed -i 's/"metadata":{"model":"m"}/"metadata":{}/g' src/interchange/lossless_tests.rs
