# Embedded admin UI

> Source: ORIGINAL_BRIEF.md §9

## 9. Embedded admin interface

Responsive browser UI served directly by the Pi, covering:

- Login and initial admin setup.
- Device overview, connectivity, uptime, software/firmware and communication status.
- Live readings and available verified controls.
- Register catalog, search, verification status, raw and decoded values.
- Wi-Fi selection, connection diagnostics, device settings.
- Updates, restart/recovery, sanitized support bundle, protected debug view.

This is a maintenance interface, **not** the full household dashboard. A separate React/Chromium dashboard on an Android tablet will consume the API, potentially via a separate Linux server for history and analytics.
