# bymm storage format — version 1

Everything is **little-endian**, whatever the CPU or the executable's own endianness.
All sizes are in bytes.

```text
file = [ original binary ][ region ][ footer ]
region = header entry*
```

The original binary is never modified: the file always starts with its exact original bytes.

## Header — 32 bytes, at the start of the region

| Offset | Size | Field | Meaning |
|---|---|---|---|
| 0 | 4 | magic | `"BYMM"` |
| 4 | 4 | version | `1` |
| 8 | 8 | entry_count | number of entries |
| 16 | 8 | limit | maximum size of header + entries + footer; `0xFFFF_FFFF_FFFF_FFFF` = unlimited |
| 24 | 4 | header_crc | CRC-32 of bytes 0..24 |
| 28 | 4 | reserved | `0` |

## Entry — 16 bytes + key + data

| Offset | Size | Field | Meaning |
|---|---|---|---|
| 0 | 4 | key_len | 1 ..= 1024 |
| 4 | 8 | data_len | any value (0 allowed) |
| 12 | 4 | crc | CRC-32 of `key ‖ data` |
| 16 | key_len | key | UTF-8, unique in the storage |
| 16+key_len | data_len | data | raw bytes |

Entries follow each other with no padding. A writer emits them sorted by key.

## Footer — 16 bytes, the very last bytes of the file

| Offset | Size | Field | Meaning |
|---|---|---|---|
| 0 | 8 | region_len | size of header + entries (footer excluded) |
| 8 | 8 | magic | `"BYMMEND\0"` |

## CRC-32

IEEE 802.3 (polynomial `0xEDB88320` reflected, init `0xFFFFFFFF`, final xor `0xFFFFFFFF`) —
the one of zip, PNG and gzip. `CRC-32("123456789") = 0xCBF43926`.

## Finding the storage

1. If the file is shorter than 16 bytes → no storage.
2. Read the last 16 bytes. If bytes 8..16 are not `"BYMMEND\0"` → no storage.
3. `region_len` must satisfy `32 ≤ region_len ≤ file_size − 16`, else the storage is **corrupt**.
4. `region_start = file_size − 16 − region_len`. The header there must have the right magic, a known
   version and a valid `header_crc`, else the storage is **corrupt** (or `UnsupportedVersion`).
5. Walk `entry_count` entries from `region_start + 32`. Every entry must fit inside the region,
   keys must be non-empty, valid UTF-8 and unique, and the walk must end exactly at the end of the region.
6. `region_start` is the length of the original binary.

Data checksums are verified when a value is read (or by `verify`), not when the index is built.

## Limit

`limit` bounds the **whole storage**: `32 + Σ(16 + key_len + data_len) + 16 ≤ limit`.
The smallest meaningful limit is 48 (an empty storage). A reader must accept a storage that is
bigger than its limit (the limit may have been lowered); a writer must not make it bigger.

## Writing

A writer never edits the file in place:

1. Create a temp file in the same directory.
2. Copy the original binary (`region_start` bytes, or the whole file if there was no storage).
3. Write header, entries, footer.
4. `fsync`, copy the permission bits of the original, atomically rename over the original.

## Example

`printf ORIGINAL > tiny; bymm limit tiny 1024; bymm set tiny hi yo` produces these 76 bytes:

```text
0000  4f 52 49 47 49 4e 41 4c                          "ORIGINAL"   original binary (8)
0008  42 59 4d 4d  01 00 00 00                         "BYMM"  version 1
0010  01 00 00 00 00 00 00 00                          entry_count = 1
0018  00 04 00 00 00 00 00 00                          limit = 1024
0020  75 75 a0 98  00 00 00 00                         header_crc, reserved
0028  02 00 00 00                                      key_len = 2
002c  02 00 00 00 00 00 00 00                          data_len = 2
0034  29 4e 20 bb                                      crc("hi" ‖ "yo")
0038  68 69                                            "hi"
003a  79 6f                                            "yo"
003c  34 00 00 00 00 00 00 00                          region_len = 52
0044  42 59 4d 4d 45 4e 44 00                          "BYMMEND\0"
```

## Compatibility

Readers must reject a `version` they do not know (`UnsupportedVersion`). The `reserved` header field
is written as 0 and ignored by version 1 readers. Format 0.1.x (a standalone file starting with
`"BYMM"`, 16-byte header) is a different, retired format.
