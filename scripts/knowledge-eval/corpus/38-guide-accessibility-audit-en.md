# Web Accessibility WCAG AA Audit Checklist

## Keyboard Navigation & Focus Management

Every interactive control, modal dialog, dropdown menu, and tab panel must be operable solely via keyboard using `Tab`, `Shift+Tab`, `Enter`, `Space`, and arrow keys. Opening a modal must trap focus inside the dialog container, and pressing `Escape` must dismiss it and return focus cleanly to the originating trigger button.

## Screen Reader Semantics & Live Regions

Use native semantic elements (`<button>`, `<nav>`, `<main>`) before reaching for ARIA attributes. Dynamic status banners and toast notifications must announce updates through `aria-live="polite"` (or `assertive` for blocking errors) without stealing keyboard focus from the user's current input field.
