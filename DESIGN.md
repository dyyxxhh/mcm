# MCM Web UI Design System

Adapted from getdesign `ollama` guidance. Minimal, documentation-first, high-contrast black-and-white with pill-shaped interactive elements.

## Tokens

### Colors

| Token | Value | Usage |
|---|---|---|
| `--color-primary` | `#000000` | Primary buttons, active nav, key text |
| `--color-on-primary` | `#ffffff` | Text on primary buttons |
| `--color-ink` | `#000000` | Headings, primary text |
| `--color-ink-deep` | `#090909` | Pressed primary button background |
| `--color-charcoal` | `#525252` | Secondary list text, muted labels |
| `--color-body` | `#737373` | Body copy, descriptions |
| `--color-mute` | `#a3a3a3` | Placeholders, disabled text, captions |
| `--color-canvas` | `#ffffff` | Page background |
| `--color-surface-soft` | `#fafafa` | Snippet/code backgrounds, empty-state fill |
| `--color-surface-card` | `#ffffff` | Cards on canvas |
| `--color-hairline` | `#e5e5e5` | Borders, dividers |
| `--color-hairline-strong` | `#d4d4d4` | Stronger input borders |
| `--color-surface-dark` | `#171717` | Error/destructive banners, inverted CTA |
| `--color-on-dark` | `#ffffff` | Text on dark surfaces |
| `--color-on-dark-mute` | `rgba(255,255,255,0.7)` | Secondary text on dark surfaces |
| `--color-focus-ring` | `rgba(59,130,246,0.5)` | Browser focus ring |
| `--color-error` | `#dc2626` | Error text (semantic override) |
| `--color-error-bg` | `#fef2f2` | Error banner background (semantic override) |
| `--color-success` | `#16a34a` | Success states (semantic override) |
| `--color-success-bg` | `#f0fdf4` | Success banner background (semantic override) |

### Typography

| Token | Font | Size | Weight | Line Height | Use |
|---|---|---|---|---|---|
| `--text-display` | SF Pro Rounded, system-ui | 36px | 500 | 1.11 | Page hero headline |
| `--text-display-sm` | SF Pro Rounded, system-ui | 28px | 500 | 1.15 | Mobile hero |
| `--text-heading-lg` | SF Pro Rounded, system-ui | 24px | 600 | 1.33 | Section titles |
| `--text-heading-md` | ui-sans-serif | 20px | 500 | 1.4 | Card titles |
| `--text-heading-sm` | ui-sans-serif | 18px | 500 | 1.56 | Form section labels |
| `--text-body` | ui-sans-serif | 16px | 400 | 1.5 | Body, form labels |
| `--text-body-strong` | ui-sans-serif | 16px | 500 | 1.5 | Emphasis, package names |
| `--text-body-sm` | ui-sans-serif | 14px | 400 | 1.43 | Meta, version, owner |
| `--text-caption` | ui-sans-serif | 12px | 400 | 1.33 | Footer, tiny meta |
| `--text-code` | ui-monospace | 14px | 400 | 1.43 | Install commands, JSON |

### Spacing

| Token | Value |
|---|---|
| `--space-xs` | 4px |
| `--space-sm` | 8px |
| `--space-md` | 12px |
| `--space-lg` | 16px |
| `--space-xl` | 24px |
| `--space-xxl` | 32px |
| `--space-section` | 64px |
| `--space-section-sm` | 48px |

### Border Radius

| Token | Value | Use |
|---|---|---|
| `--radius-full` | 9999px | Buttons, inputs, pills, badges |
| `--radius-lg` | 12px | Cards, terminal/code blocks |
| `--radius-md` | 8px | Small panels |

## Components

### Button Primary
- Background: `--color-primary`
- Text: `--color-on-primary`
- Padding: 8px 20px
- Height: 40px
- Border radius: `--radius-full`
- Font: `--text-body-sm` weight 500
- Hover: background `--color-ink-deep`
- Disabled: background `--color-surface-soft`, text `--color-mute`

### Button Secondary
- Background: `--color-canvas`
- Text: `--color-ink`
- Border: 1px solid `--color-hairline-strong`
- Padding: 8px 20px
- Height: 40px
- Border radius: `--radius-full`

### Button Small
- Padding: 4px 12px
- Height: 32px
- Font: `--text-caption` weight 500

### Button Destructive
- Background: `--color-error-bg`
- Text: `--color-error`
- Border: 1px solid `--color-error`
- Border radius: `--radius-full`

### Text Input / Textarea
- Background: `--color-canvas`
- Border: 1px solid `--color-hairline`
- Border radius: `--radius-full` for inputs, `--radius-lg` for textarea
- Padding: 12px 16px
- Font: `--text-body`
- Focus: border `--color-ink`, ring `--color-focus-ring`
- Placeholder color: `--color-mute`

### Install Snippet
- Background: `--color-surface-soft`
- Text: `--color-ink`
- Font: `--text-code`
- Padding: 12px 20px
- Border radius: `--radius-full`
- Display: flex row with copy button at right

### Card
- Background: `--color-surface-card`
- Border: 1px solid `--color-hairline`
- Border radius: `--radius-lg`
- Padding: `--space-xl`

### Error Banner
- Background: `--color-error-bg`
- Text: `--color-error`
- Border: 1px solid `--color-error`
- Border radius: `--radius-lg`
- Padding: `--space-md` `--space-lg`

### Empty State
- Centered text in `--color-body`
- Optional soft background panel
- Link to primary action

### Spinner
- 20px circle, 2px border `--color-hairline`, top border `--color-primary`
- Animation: rotate 1s linear infinite
- No emoji

## Layout

- Page max width: 720px reading column for auth/login; 960px for dashboard.
- Outer padding: 16px mobile, 24px tablet, 32px desktop.
- Section vertical spacing: `--space-section` desktop, `--space-section-sm` mobile.
- Stack layout with `--space-lg` between form fields.

## Responsive

- Mobile (< 640px): single column, hero text `--text-display-sm`, nav collapses to simple header.
- Tablet (640px–1024px): 2-column where applicable.
- Desktop (1280px+): max-width containers centered.

## Iconography

- No emojis. Use inline SVG or CSS shapes only.
- Copy icon: two overlapping rectangles SVG.
- Trash icon: bin SVG for delete.
- Pencil icon: pencil SVG for edit.
- Spinner: CSS animated ring.

## Do / Don't

- Do use design tokens for every color, spacing, font, and radius value.
- Do keep surfaces flat; no drop shadows.
- Do use pill buttons and rounded inputs.
- Do show loading spinners and clear error messages.
- Don't use emoji as icons.
- Don't hardcode magic numbers in CSS.
- Don't introduce gradients or decorative backgrounds.
