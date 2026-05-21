# AEBC — Aether Bytecode File Format

| Offset | Size | Content |
|--------|------|---------|
| 0      | 8 B  | Magic: `AEBC\0\0\0\x01` (ASCII tag + 3 reserved bytes + version `1`) |
| 8      | N B  | bincode-serialized `aether_bc::Program` (little-endian, length-prefixed sequences) |

Identify with `file(1)`: the first 8 bytes are `41 45 42 43 00 00 00 01`.
