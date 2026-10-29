# LLD: Named Call Arguments (Phase 1)

**Status:** Implemented (Phase 1)

## Syntax

```grf
add(a=1, b=2);
add(1, b=2);       // positional then named
log.info(message="hi", save_to="");
```

Rules:

- `name=value` in a call/new/super argument list binds by parameter name.
- Once a named argument appears, later arguments must also be named.
- Duplicate names and unknown names are errors (`GRF-SEM-037`).
- Missing required parameters are errors; defaults still fill omitted trailing params.
- Built-ins that are not real parameter lists (`range`, `lengthOf`, `typeOf`) reject named args.
- Keyword tokens are allowed as names when followed by `=` (so `type="…"` works).

## Out of scope (later)

- `...args` capture / spread
- `{a,b}=args` destructuring
- Keyword-only parameters

## Integration

Parser stores `CallArg { name?, value }` on `CallExpr` / `NewExpr` / `SuperStmt`.
Semantic `check_call_args` binds positionals then names into parameter slots before type-checking.
