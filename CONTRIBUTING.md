# Contributing to Forkit

Thank you for your interest in contributing to Forkit! This document provides guidelines and instructions for contributing.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Project Structure](#project-structure)
- [Making Changes](#making-changes)
- [Testing](#testing)
- [Coding Standards](#coding-standards)
- [Commit Messages](#commit-messages)
- [Pull Request Process](#pull-request-process)

## Code of Conduct

By participating in this project, you agree to maintain a respectful and inclusive environment. Be considerate of others and follow standard open-source community guidelines.

## Getting Started

1. Fork the repository on GitHub
2. Clone your fork locally:
   ```bash
   git clone https://github.com/yourusername/forkit.git
   cd forkit
   ```
3. Install dependencies (see [Development Setup](#development-setup))
4. Create a branch for your changes:
   ```bash
   git checkout -b feature/your-feature-name
   ```

## Development Setup

### Prerequisites

- Rust 1.70 or later
- SDL2 development libraries

#### Installing Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

#### Installing SDL2

**Ubuntu/Debian:**
```bash
sudo apt-get install libsdl2-dev libsdl2-ttf-dev libsdl2-image-dev
```

**Fedora/RHEL:**
```bash
sudo dnf install SDL2-devel SDL2_ttf-devel SDL2_image-devel
```

**macOS:**
```bash
brew install sdl2 sdl2_ttf sdl2_image
```

**Windows:**
Download SDL2 development libraries from [libsdl.org](https://www.libsdl.org/) and set the `SDL2_DIR` environment variable.

### Building

```bash
cargo build
```

### Running Tests

```bash
cargo test
```

### Running the Browser

```bash
cargo run [URL]
```

### Code Style

- Follow Rust standard naming conventions
- Use `snake_case` for functions and variables
- Use `CamelCase` for types and traits
- Add documentation comments for public APIs
- Keep functions focused and reasonably sized
- Handle errors appropriately (avoid unwrap in production code)

### Example Code Style

```rust
/// Parse a CSS color value.
///
/// # Arguments
/// * `value` - The CSS color string (e.g., "#ff0000", "rgb(255,0,0)")
///
/// # Returns
/// An RGB tuple if parsing succeeds, or None if the value is invalid.
pub fn parse_color(value: &str) -> Option<[u8; 3]> {
    // Implementation
}
```

## Testing

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run tests with output
cargo test -- --nocapture
```

### Writing Tests

- Place unit tests in the same file using `#[cfg(test)]` modules
- Place integration tests in the `tests/` directory
- Test edge cases and error conditions
- Use descriptive test names

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_color() {
        assert_eq!(parse_color("#ff0000"), Some([255, 0, 0]));
    }

    #[test]
    fn test_parse_invalid_color() {
        assert_eq!(parse_color("invalid"), None);
    }
}
```

## Coding Standards

### Error Handling

- Use `Result<T, E>` for fallible operations
- Provide meaningful error messages
- Use `Option<T>` for nullable values
- Avoid panicking in library code

```rust
// Good
pub fn fetch_url(url: &str) -> Result<(String, String), String> {
    // Implementation
}

// Avoid
pub fn fetch_url(url: &str) -> (String, String) {
    // Panics on error
}
```

### Memory Safety

- Follow Rust ownership rules strictly
- Use lifetimes appropriately
- Prefer borrowing over cloning
- Document any unsafe code blocks

### Performance Considerations

- Avoid unnecessary allocations
- Use iterators instead of loops where appropriate
- Cache expensive computations
- Profile before optimizing

## Commit Messages

Format:
```
type(scope): brief description

Longer description if needed.

Fixes #issue-number
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting, etc.)
- `refactor`: Code refactoring
- `test`: Adding or updating tests
- `chore`: Maintenance tasks

Examples:
```
feat(css): add flexbox layout support

Implement flex container and flex item properties including
flex-direction, justify-content, and align-items.

Fixes #42
```

```
fix(parser): handle unclosed tags correctly

Previously, unclosed tags could cause infinite loops in the parser.
Now properly tracked and closed at document end.
```

## Pull Request Process

1. **Create a Branch**
   ```bash
   git checkout -b feature/your-feature
   ```

2. **Make Changes**
   - Write clean, documented code
   - Add tests for new functionality
   - Update documentation if needed

3. **Test Your Changes**
   ```bash
   cargo test
   cargo clippy
   cargo fmt -- --check
   ```

4. **Commit Your Changes**
   ```bash
   git add .
   git commit -m "type(scope): description"
   ```

5. **Push to Your Fork**
   ```bash
   git push origin feature/your-feature
   ```

6. **Open a Pull Request**
   - Go to the original repository
   - Click "New Pull Request"
   - Select your branch
   - Fill in the PR template

7. **Address Review Feedback**
   - Make requested changes
   - Push new commits to the same branch
   - Respond to all comments

### PR Checklist

- [ ] Code compiles without errors
- [ ] All tests pass
- [ ] New code has tests
- [ ] Documentation updated if needed
- [ ] Commit messages follow the format
- [ ] No unnecessary dependencies added

## Getting Help

- Open an issue for bugs or feature requests
- Check existing issues before creating new ones
- Provide detailed information (Rust version, OS, error messages)

## Recognition

Contributors will be acknowledged in the project documentation. Thank you for helping improve Forkit!