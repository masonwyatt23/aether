# Aether AST JSON Schema

Version: **0.1**

The `aether ast --json [--pretty] <file>` command emits a stable, schema-versioned JSON representation of the parsed AST.

---

## Top-level envelope

```json
{
  "version": "0.1",
  "module": <Module>
}
```

| Field     | Type     | Description                               |
|-----------|----------|-------------------------------------------|
| `version` | `string` | Schema version. Currently always `"0.1"`. |
| `module`  | `Module` | The parsed module object.                 |

---

## Span

Every AST node carries a `span` field with byte-offset source location.

```json
{ "file": 0, "start": 10, "end": 25 }
```

| Field   | Type  | Description                                          |
|---------|-------|------------------------------------------------------|
| `file`  | `u32` | Index into the `SourceMap` (opaque per compilation). |
| `start` | `u32` | Inclusive start byte offset in the source file.      |
| `end`   | `u32` | Exclusive end byte offset in the source file.        |

---

## Enum representation

Serde's default **externally-tagged** representation is used for all enum types:

```json
{ "VariantName": <payload> }
```

For unit variants (no payload) the value is `null`:

```json
{ "Unit": null }
```

Named-field struct variants serialize as objects under the variant key.  
Tuple variants serialize as arrays under the variant key.

---

## Module

```json
{
  "name": "hello" | null,
  "doc": "## Module doc comment." | null,
  "decls": [ <Decl>, ... ],
  "span": <Span>
}
```

---

## Decl

Externally-tagged enum. Variants:

| Tag          | Payload type    |
|--------------|-----------------|
| `"Fn"`       | `FnDecl`        |
| `"Let"`      | `LetDecl`       |
| `"TypeAlias"`| `TypeAliasDecl` |
| `"Import"`   | `ImportDecl`    |
| `"Tool"`     | `ToolDecl`      |

### Example — `Decl::Fn`

```json
{
  "Fn": {
    "name": "add",
    "generics": [],
    "params": [
      { "name": "a", "ty": { "Con": ["Int", <Span>] }, "default": null, "span": <Span> },
      { "name": "b", "ty": { "Con": ["Int", <Span>] }, "default": null, "span": <Span> }
    ],
    "ret": { "Con": ["Int", <Span>] },
    "effects": { "effects": [], "tail": null },
    "spec": { "requires": [], "ensures": [], "effects": null },
    "body": { "Bin": [{ "Add": null }, { "Var": ["a", <Span>] }, { "Var": ["b", <Span>] }, <Span>] },
    "doc": null,
    "no_prov": false,
    "span": <Span>
  }
}
```

---

## Expr

Externally-tagged enum. Key variants:

| Tag         | Payload                                        |
|-------------|------------------------------------------------|
| `"Lit"`     | `[<Lit>, <Span>]`                              |
| `"Var"`     | `[<string>, <Span>]`                           |
| `"Bin"`     | `[<BinOp>, <Expr>, <Expr>, <Span>]`            |
| `"Un"`      | `[<UnOp>, <Expr>, <Span>]`                     |
| `"Call"`    | `{ "callee": <Expr>, "args": [...], "span": <Span> }` |
| `"Lambda"`  | `{ "params": [...], "ret": <Type>?, "body": <Expr>, "span": <Span> }` |
| `"Let"`     | `{ "pat": <Pattern>, "ty": <Type>?, "value": <Expr>, "body": <Expr>, "span": <Span> }` |
| `"If"`      | `{ "cond": <Expr>, "then_branch": <Expr>, "else_branch": <Expr>, "span": <Span> }` |
| `"Block"`   | `{ "stmts": [...], "tail": <Expr>?, "span": <Span> }` |
| `"Match"`   | `{ "scrutinee": <Expr>, "arms": [...], "span": <Span> }` |
| `"Annot"`   | `{ "expr": <Expr>, "ty": <Type>, "span": <Span> }` |

### Example — `Expr::Bin`

```json
{
  "Bin": [
    { "Add": null },
    { "Var": ["x", { "file": 0, "start": 4, "end": 5 }] },
    { "Lit": [{ "Int": 1 }, { "file": 0, "start": 8, "end": 9 }] },
    { "file": 0, "start": 4, "end": 9 }
  ]
}
```

### Example — `Expr::Call`

```json
{
  "Call": {
    "callee": { "Var": ["print", { "file": 0, "start": 2, "end": 7 }] },
    "args": [
      {
        "name": null,
        "value": { "Lit": [{ "Str": "hello" }, { "file": 0, "start": 8, "end": 15 }] },
        "span": { "file": 0, "start": 8, "end": 15 }
      }
    ],
    "span": { "file": 0, "start": 2, "end": 16 }
  }
}
```

---

## Type

Externally-tagged enum. Key variants:

| Tag          | Payload                                                          |
|--------------|------------------------------------------------------------------|
| `"Var"`      | `[<string>, <Span>]`                                             |
| `"Con"`      | `[<TyCon>, <Span>]`                                              |
| `"Fun"`      | `{ "params": [...], "ret": <Type>, "effects": <EffectRow>, "span": <Span> }` |
| `"Refined"`  | `{ "base": <Type>, "refinement": <Refinement>, "span": <Span> }` |
| `"Tuple"`    | `[<Type[]>, <Span>]`                                             |
| `"List"`     | `[<Type>, <Span>]`                                               |
| `"Option"`   | `[<Type>, <Span>]`                                               |
| `"Generic"`  | `{ "name": <string>, "args": [...], "span": <Span> }`            |

---

## TyCon

Unit-variant enum serialized as an externally-tagged object with `null` payload:

```json
{ "Int": null }
{ "Float": null }
{ "Bool": null }
{ "Str": null }
{ "Bytes": null }
{ "Unit": null }
{ "ModuleSurface": null }
{ "ProvChain": null }
```

---

## Effect / EffectRow

`Effect` is an enum: unit variants (`IO`, `Net`, `FS`, `State`, `Rand`, `Async`, `Throw`) serialize as `{ "IO": null }` etc.; `Custom("name")` serializes as `{ "Custom": "name" }`.

`EffectRow` is a struct:

```json
{ "effects": [{ "IO": null }], "tail": null }
```

---

## Pattern

Externally-tagged enum. Variants:

| Tag      | Payload                                        |
|----------|------------------------------------------------|
| `"Wild"` | `<Span>`                                       |
| `"Var"`  | `[<string>, <Span>]`                           |
| `"Lit"`  | `[<Lit>, <Span>]`                              |
| `"Tuple"`| `[<Pattern[]>, <Span>]`                        |
| `"Record"`| `[<[string, Pattern][]>, <Span>]`             |
| `"Ctor"` | `{ "name": <string>, "args": [...], "span": <Span> }` |

---

## Notes

- `ProvChain`, `ProvOp`, and `ProvNode` are **not serialized** — they are runtime-only types. No AST node holds them directly.
- `SourceMap` is **not included** in the output — spans use opaque `file` indices.
- The schema version (`"0.1"`) will be bumped on any breaking change to the envelope or field names.
- All `Box<T>` fields serialize identically to `T` (serde transparent boxing).
- `Option<T>` fields serialize as `null` when absent.
