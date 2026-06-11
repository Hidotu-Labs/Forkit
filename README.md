# Forkit - A Minimal Web Browser in Rust

Forkit is a lightweight web browser built from scratch using Rust and SDL2. It implements core browser components including HTML parsing, CSS styling, layout engines, and basic JavaScript execution.

## Features

### HTML Support
- Full HTML5 parsing and DOM tree construction
- Support for semantic elements (header, footer, nav, article, section, aside)
- Form elements (input, textarea, button, select)
- Tables with proper layout
- Lists (ordered and unordered)
- Media placeholders (img, video, audio, canvas)
- Links and navigation

### CSS Support
- Color formats: hex, rgb, rgba, hsl, hsla, named colors
- Typography: font-size, font-weight, font-style, text-align, text-decoration, text-transform
- Box model: margin, padding, border, border-radius
- Sizing: width, height, max-width, min-width, max-height, min-height
- Background: background-color, background-image, background-size, background-repeat, background-position
- Display modes: block, inline, inline-block, none
- Visibility and overflow control
- CSS functions: calc(), clamp(), min(), max()
- CSS selectors: tag, class, id, compound, descendant, child, adjacent sibling, attribute
- CSS cascade with specificity calculation

### JavaScript Support
- Variable declarations: var, let, const
- Arithmetic operators: +, -, *, /, %
- Comparison operators: ==, !=, <, >, <=, >=
- Logical operators: &&, ||, !
- String concatenation
- Console output: console.log(), console.warn()
- Comments: single-line and multi-line

### Browser Features
- Multi-tab support with tab bar
- Address bar with URL input
- Navigation controls (back, forward, reload)
- Scroll support with custom scrollbar
- Background page loading
- HTTP/HTTPS support with automatic HTTPS upgrade
- Character encoding support (UTF-8, ISO-8859-9, Windows-1252, Windows-1250)
- URL resolution and normalization

## Keyboard Shortcuts

### Navigation
- `Ctrl + T` - Open new tab
- `Ctrl + W` - Close current tab
- `Ctrl + Tab` - Switch to next tab
- `Ctrl + Shift + Tab` - Switch to previous tab
- `Ctrl + 1-9` - Switch to specific tab
- `Alt + Left` - Go back
- `Alt + Right` - Go forward
- `Ctrl + R` or `F5` - Reload page

### Scrolling
- `Up/Down` or `J/K` - Scroll by step
- `Page Up/Page Down` - Scroll by page
- `Home` - Scroll to top

### Address Bar
- `Ctrl + L` or `F6` - Focus address bar
- `Escape` - Unfocus address bar
- `Ctrl + A` - Select all text in address bar

### General
- `Escape` (outside input fields) - Quit browser
- `Q` - Quit browser

## Installation

### Prerequisites

1. Rust (1.70 or later)
2. SDL2 development libraries

#### Installing SDL2

**Ubuntu/Debian:**
```bash
sudo apt-get install libsdl2-dev libsdl2-ttf-dev libsdl2-image-dev
```

**Fedora/RHEL:**
```bash
sudo dnf install SDL2-devel SDL2_ttf-devel SDL2_image-devel
```

**macOS (Homebrew):**
```bash
brew install sdl2 sdl2_ttf sdl2_image
```

**Windows:**
1. Download SDL2 development libraries from [libsdl.org](https://www.libsdl.org/)
2. Extract to a directory
3. Set environment variables:
   ```powershell
   set SDL2_DIR=path\to\sdl2
   ```

### Building

```bash
git clone https://github.com/yourusername/forkit.git
cd forkit
cargo build --release
```

### Running

```bash
cargo run --release [URL]
```

If no URL is provided, the browser loads `assets/test.html` by default.

### Examples

```bash
# Load a web page
cargo run --release https://example.com

# Load a local file
cargo run --release file:///path/to/file.html

# Load the test page
cargo run --release
```

### Key Components

1. **HTML Parser** - Tokenizes and builds DOM tree from HTML source
2. **CSS Engine** - Parses stylesheets, computes cascade, applies styles
3. **Layout Engine** - Calculates positions and sizes for all elements
4. **Renderer** - Paints elements to SDL2 canvas
5. **JavaScript Interpreter** - Executes inline and external scripts
6. **Network Layer** - Fetches resources over HTTP/HTTPS

## Development Status

This project is in active development. It serves as both a practical tool and an educational resource for understanding browser internals.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

This project is licensed under the Apache 2.0 License - see the [LICENSE](LICENSE) file for details.