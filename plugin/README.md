# Aragon Studio plugin

The Roblox Studio side of [Aragon](../README.md). It connects Studio to the Aragon hub on your PC and nothing else: open a place and it says hello, follows the hub's instructions and reconnects on its own. Every setting lives in the dashboard.

You don't need to install this by hand. `aragon.exe` embeds the plugin and installs it from the welcome screen, or run `aragon plugin install`.

## Develop

```bash
rokit install            # from the repo root
cd plugin
scripts/install          # wally install + package types
rojo build default.project.json --output Aragon.rbxm
selene src
```

`src/Lib/Dom/database.luau` is generated from `scripts/database.msgpack` by `scripts/gen_database.py`, because Rojo doesn't pick up `.msgpack` files.
