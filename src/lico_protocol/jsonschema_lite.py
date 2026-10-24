"""A small JSON Schema subset validator (no third-party package).

Supports what the provider's generated schema uses: type (also a list), enum, const,
properties, required, items, $ref to #/definitions, oneOf, anyOf and allOf. Unknown
keywords, formats and additionalProperties are ignored, so it is lenient by design: it
catches missing required fields, wrong types and unknown enum or constant values.
"""

from __future__ import annotations


def _type_ok(value, kind: str) -> bool:
    if kind == "null":
        return value is None
    if kind == "boolean":
        return isinstance(value, bool)
    if kind == "integer":
        return isinstance(value, int) and not isinstance(value, bool)
    if kind == "number":
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    if kind == "string":
        return isinstance(value, str)
    if kind == "array":
        return isinstance(value, list)
    if kind == "object":
        return isinstance(value, dict)
    return True


def validate(value, schema: dict, root: dict, path: str = "$") -> list[str]:
    if "$ref" in schema:
        name = schema["$ref"].rsplit("/", 1)[-1]
        target = root.get("definitions", {}).get(name)
        if target is None:
            return [f"{path}: unresolved reference {schema['$ref']}"]
        return validate(value, target, root, path)
    errors: list[str] = []
    if "allOf" in schema:
        for part in schema["allOf"]:
            errors += validate(value, part, root, path)
    for key in ("oneOf", "anyOf"):
        if key in schema:
            if not any(not validate(value, option, root, path) for option in schema[key]):
                errors.append(f"{path}: matches none of the {key} alternatives")
    if "type" in schema:
        kinds = schema["type"] if isinstance(schema["type"], list) else [schema["type"]]
        if not any(_type_ok(value, k) for k in kinds):
            return errors + [f"{path}: expected {kinds}"]
    if "enum" in schema and value not in schema["enum"]:
        errors.append(f"{path}: {value!r} is not one of {schema['enum']}")
    if "const" in schema and value != schema["const"]:
        errors.append(f"{path}: expected {schema['const']!r}")
    if isinstance(value, dict):
        for name in schema.get("required", []):
            if name not in value:
                errors.append(f"{path}.{name}: missing")
        for name, sub in schema.get("properties", {}).items():
            if name in value:
                errors += validate(value[name], sub, root, f"{path}.{name}")
    if isinstance(value, list) and "items" in schema:
        for index, item in enumerate(value):
            errors += validate(item, schema["items"], root, f"{path}[{index}]")
    return errors
