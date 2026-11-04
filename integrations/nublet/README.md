# Polyph for Nublet

Polyph provides its status to Nublet (and to any other widget or ambient
display) through the daemon's read-only **status provider**. No plugin code
runs inside Polyph, and nothing beyond states, names, counts and voice colors
leaves it.

1. On the computer running Polyph:

   ```bash
   polyph status --token
   ```

   It prints the status token and a widget URL like
   `http://127.0.0.1:47850/status/widget?token=st_…`.

2. In Nublet, add a provider with that URL. `provider.json` here describes it:
   the polled summary (`/status/widget`, every 5 seconds) and the live stream
   (`/status/events`, Server-Sent Events).

3. To stop a display reading your status, run `polyph status --rotate-token`.

The document formats are in `docs/status-feed.md`. If Nublet expects a
different document shape, map it from `/status/widget` (a ready-to-draw
summary) rather than from the raw feed.
