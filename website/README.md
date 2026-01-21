# Otto Documentation Website

This directory contains the Hugo static site for Otto's documentation.

## Quick Start

```sh
# Install Hugo
sudo apt install hugo

# Run development server (auto-rebuilds on changes)
./dev.sh
```

Visit http://localhost:1313 when running the dev server.

## Scripts

- `./dev.sh` - Development server with auto-rebuild (recommended)
- `./build-docs.sh` - Manually build merged documentation
- `./watch-docs.sh` - Watch docs/ folder and auto-rebuild
- `hugo server` - Run Hugo without auto-rebuild
- `hugo` - Build static site to public/

## Editing Documentation

1. Edit markdown files in `../docs/user/` or `../docs/developer/`
2. If using `./dev.sh`, changes auto-rebuild (checks every 2 seconds)
3. Hugo will auto-reload the browser

## Structure

- `../docs/user/` - User guide markdown files
- `../docs/developer/` - Developer guide markdown files
- `content/_index.md` - Auto-generated user guide (gitignored)
- `content/developer.md` - Auto-generated developer guide (gitignored)
- `layouts/` - HTML templates
- `assets/` - CSS, JS, images
- `hugo.toml` - Hugo configuration
- `public/` - Generated site (gitignored)

## Customization

The theme is based on [clig.dev](https://clig.dev/). Edit `assets/css/main.css` and `layouts/` files to customize the appearance.
