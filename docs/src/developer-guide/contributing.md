# Contributing

Welcome! This guide helps you contribute to exrs.

## Getting Started

### Clone the Repository

```bash
git clone https://github.com/johannesvollmer/exrs.git
cd exrs
```

### Build

```bash
cargo build
```

### Run Tests

```bash
cargo test
```

### Run Benchmarks

```bash
cargo bench
```

## Project Structure

```
exrs/
├── src/                    # Library source
│   ├── lib.rs             # Crate root
│   ├── block/             # Low-level block I/O
│   ├── compression/       # Compression algorithms
│   ├── image/             # High-level API
│   ├── meta/              # Metadata types
│   └── ...
├── examples/              # Usage examples
├── tests/                 # Integration tests
│   ├── images/            # Test images
│   │   ├── valid/         # Valid EXR files
│   │   └── invalid/       # Invalid/fuzzed files
│   └── *.rs               # Test files
├── benches/               # Benchmarks
├── docs/                  # This documentation
└── specification/         # OpenEXR specs (PDFs)
```

## Development Workflow

### 1. Create a Branch

```bash
git checkout -b feature/my-feature
```

### 2. Make Changes

Follow the code style guidelines below.

### 3. Run Tests

```bash
# All tests
cargo test

# Specific test
cargo test test_name

# With output
cargo test -- --nocapture
```

### 4. Run Clippy

```bash
cargo clippy --all-targets --all-features
```

### 5. Format Code

```bash
cargo fmt
```

### 6. Submit PR

Push your branch and create a pull request.

## Code Style

### Rust Conventions

- Follow Rust naming conventions
- Use `rustfmt` for formatting
- Address all `clippy` warnings

### Documentation

- Document public items with `///`
- Include examples in doc comments
- Use `#[doc(hidden)]` sparingly

### Error Handling

```rust
// Use the crate's Result type
use crate::error::{Error, Result};

// Return descriptive errors
fn parse_data(bytes: &[u8]) -> Result<Data> {
    if bytes.len() < 4 {
        return Err(Error::Invalid("data too short".into()));
    }
    // ...
}
```

### Safety

- No `unsafe` code (library uses `#[forbid(unsafe_code)]`)
- Validate all external input
- Use bounds-checked operations

## Adding Features

### New Compression Method

1. Create `src/compression/method.rs`
2. Add variant to `Compression` enum
3. Add dispatch in `compression/mod.rs`
4. Add tests
5. Update documentation

### New Attribute Type

1. Add variant to `AttributeValue` enum in `meta/attribute.rs`
2. Implement `Data` trait (read/write)
3. Add tests
4. Update documentation

### New Reader/Writer Option

1. Add builder method in appropriate module
2. Add type state if needed
3. Update integration tests
4. Add example if user-facing

## Testing

### Unit Tests

Place in the same file:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_something() {
        // ...
    }
}
```

### Integration Tests

Place in `tests/`:

```rust
// tests/my_feature.rs
use exr::prelude::*;

#[test]
fn test_feature() {
    // ...
}
```

### Test Images

- `tests/images/valid/` - Valid EXR files
- `tests/images/invalid/` - Invalid/edge case files

### Fuzz Testing

```bash
# Run indefinitely
cargo test --package exr --test fuzz fuzz -- --exact --ignored
```

## Benchmarks

Located in `benches/`:

```rust
// benches/read.rs
use bencher::{benchmark_group, benchmark_main, Bencher};

fn bench_read(b: &mut Bencher) {
    b.iter(|| {
        // ...
    });
}

benchmark_group!(benches, bench_read);
benchmark_main!(benches);
```

Run:
```bash
cargo bench
```

## Pull Request Guidelines

### Title

Use conventional commit format:
- `feat: Add DWAA compression`
- `fix: Handle empty deep pixels`
- `docs: Update GUIDE.md`
- `refactor: Simplify block reader`
- `test: Add roundtrip tests for deep`

### Description

Include:
1. What the PR does
2. Why it's needed
3. How to test
4. Breaking changes (if any)

### Checklist

- [ ] Tests pass
- [ ] Clippy passes
- [ ] Code formatted
- [ ] Documentation updated
- [ ] Examples updated (if needed)

## Common Tasks

### Adding a Test Image

1. Place in `tests/images/valid/` or `tests/images/invalid/`
2. Update `.gitignore` if large
3. Reference in tests

### Updating Dependencies

```bash
cargo update
cargo test
```

### Generating Docs

```bash
cargo doc --open
```

### Building for WASM

```bash
cargo build --target wasm32-unknown-unknown --no-default-features
```

## Getting Help

- Open an issue for questions
- Check existing issues/PRs
- Read the specification PDFs in `specification/`

## Code of Conduct

Be respectful and constructive. We welcome contributors of all experience levels.

## License

Contributions are licensed under BSD-3-Clause, matching the project license.
