#!/usr/bin/env python3
"""Generate a standalone HTML browser for the Rust workspace."""

from __future__ import annotations

import argparse
import html
import json
import os
import re
import tomllib
from dataclasses import dataclass, field
from pathlib import Path


IDENT = r"[A-Za-z_][A-Za-z0-9_]*"
FEATURE_RE = re.compile(r'feature\s*=\s*"([^"]+)"')
MOD_RE = re.compile(
    rf"^(?P<vis>pub(?:\([^)]*\))?\s+)?(?:unsafe\s+)?mod\s+(?P<name>{IDENT})\b"
)
USE_RE = re.compile(r"^(?P<vis>pub(?:\([^)]*\))?\s+)?use\s+(?P<path>.+?);$")
ITEM_RE = re.compile(
    rf"^(?P<vis>pub(?:\([^)]*\))?\s+)?"
    r"(?P<prefix>(?:(?:async|const|unsafe)\s+|extern\s+\"[^\"]+\"\s+)*)"
    rf"(?P<kind>struct|enum|trait|fn|const|static|type)\s+(?P<name>{IDENT})\b"
)
IMPL_RE = re.compile(
    r"^impl(?:\s*<[^>{;]*>)?\s+"
    rf"(?:(?P<trait>{IDENT}(?:::{IDENT})*)\s+for\s+)?"
    rf"(?P<target>{IDENT}(?:::{IDENT})*)"
)
MACRO_RE = re.compile(rf"^(?P<vis>pub(?:\([^)]*\))?\s+)?macro_rules!\s+(?P<name>{IDENT})\b")
ATTRIBUTE_RE = re.compile(r"^#\[(?P<body>.*)\]\s*$")
PATH_ATTR_RE = re.compile(r'#\[path\s*=\s*"([^"]+)"\]')
INCLUDE_RE = re.compile(r'^include!\s*\(\s*"([^"]+)"\s*\)\s*;')


@dataclass
class UseDecl:
    line: int
    raw: str
    visibility: str
    target: str | None = None


@dataclass
class ModDecl:
    line: int
    name: str
    visibility: str
    target: str | None = None
    path_attr: str | None = None
    inline: bool = False
    cfgs: list[str] = field(default_factory=list)


@dataclass
class IncludeDecl:
    line: int
    path: str
    target: str | None = None


@dataclass
class ItemDecl:
    line: int
    kind: str
    name: str
    visibility: str
    signature: str
    cfgs: list[str] = field(default_factory=list)
    is_test: bool = False


@dataclass
class ImplDecl:
    line: int
    target: str
    trait: str | None
    signature: str
    cfgs: list[str] = field(default_factory=list)


@dataclass
class RustModule:
    module_id: str
    crate_name: str
    source_path: Path | None
    parent: str | None
    kind: str = "source"
    line_count: int = 0
    loc: int = 0
    uses: list[UseDecl] = field(default_factory=list)
    mods: list[ModDecl] = field(default_factory=list)
    includes: list[IncludeDecl] = field(default_factory=list)
    items: list[ItemDecl] = field(default_factory=list)
    impls: list[ImplDecl] = field(default_factory=list)
    features: set[str] = field(default_factory=set)
    children: list[str] = field(default_factory=list)
    incoming: list[str] = field(default_factory=list)


@dataclass
class CrateInfo:
    package: str
    module_name: str
    root: Path
    features: list[str]
    dependencies: list[str]


def read_toml(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def crate_module_name(package_name: str) -> str:
    return package_name.replace("-", "_")


def discover_crates(root: Path) -> list[CrateInfo]:
    workspace = read_toml(root / "Cargo.toml")
    members = workspace.get("workspace", {}).get("members", [])
    crates: list[CrateInfo] = []
    for member in members:
        crate_root = (root / member).resolve()
        manifest = crate_root / "Cargo.toml"
        if not manifest.exists():
            continue
        cargo = read_toml(manifest)
        package_name = cargo.get("package", {}).get("name", crate_root.name)
        dependencies = sorted(
            set(cargo.get("dependencies", {}))
            | set(cargo.get("dev-dependencies", {}))
            | set(cargo.get("build-dependencies", {}))
        )
        crates.append(
            CrateInfo(
                package=package_name,
                module_name=crate_module_name(package_name),
                root=crate_root,
                features=sorted(cargo.get("features", {})),
                dependencies=dependencies,
            )
        )
    return crates


def rust_sources(crate: CrateInfo) -> list[Path]:
    paths: list[Path] = []
    for base in (crate.root / "src", crate.root / "tests", crate.root / "benches"):
        if base.exists():
            paths.extend(sorted(base.rglob("*.rs")))
    return sorted(paths)


def module_id_for_source(crate: CrateInfo, source: Path) -> str:
    rel = source.relative_to(crate.root)
    if rel.parts[0] == "src":
        src_rel = rel.relative_to("src")
        if src_rel.name in ("lib.rs", "main.rs") and len(src_rel.parts) == 1:
            return crate.module_name
        if src_rel.name == "mod.rs":
            parts = src_rel.parent.parts
        else:
            parts = src_rel.with_suffix("").parts
        return "::".join((crate.module_name, *parts))
    if rel.parts[0] == "tests":
        return "::".join((crate.module_name, "tests", rel.with_suffix("").name))
    if rel.parts[0] == "benches":
        return "::".join((crate.module_name, "benches", rel.with_suffix("").name))
    return "::".join((crate.module_name, *rel.with_suffix("").parts))


def parent_module(module_id: str) -> str | None:
    parts = module_id.split("::")
    if len(parts) <= 1:
        return None
    return "::".join(parts[:-1])


def strip_line_comment(line: str) -> str:
    in_string = False
    escape = False
    for index in range(len(line) - 1):
        char = line[index]
        if in_string:
            if escape:
                escape = False
            elif char == "\\":
                escape = True
            elif char == '"':
                in_string = False
            continue
        if char == '"':
            in_string = True
            continue
        if char == "/" and line[index + 1] == "/":
            return line[:index]
    return line


def sanitize_signature(line: str) -> str:
    cleaned = strip_line_comment(line).strip()
    cleaned = re.sub(r"\s+", " ", cleaned)
    if len(cleaned) > 140:
        cleaned = cleaned[:137].rstrip() + "..."
    return cleaned


def count_loc(lines: list[str]) -> int:
    in_block_comment = False
    total = 0
    for line in lines:
        text = line
        while text:
            if in_block_comment:
                end = text.find("*/")
                if end == -1:
                    text = ""
                    break
                text = text[end + 2 :]
                in_block_comment = False
                continue
            start = text.find("/*")
            if start == -1:
                text = strip_line_comment(text)
                break
            before = text[:start]
            end = text.find("*/", start + 2)
            if end == -1:
                text = before
                in_block_comment = True
                break
            text = before + text[end + 2 :]
        if text.strip():
            total += 1
    return total


def compact_visibility(visibility: str | None) -> str:
    return " ".join((visibility or "private").split())


def parse_use_path(raw_path: str) -> str:
    text = raw_path.strip().rstrip(";")
    text = re.sub(rf"\s+as\s+{IDENT}\s*$", "", text)
    text = re.sub(r"\s+", "", text)
    text = text.split("{", 1)[0]
    text = text.rstrip(":")
    return text


def parse_source(module: RustModule) -> None:
    if module.source_path is None:
        return
    lines = module.source_path.read_text(encoding="utf-8").splitlines()
    module.line_count = len(lines)
    module.loc = count_loc(lines)
    attrs: list[str] = []
    use_accumulator: list[str] = []
    use_start_line = 0

    for line_no, raw_line in enumerate(lines, start=1):
        stripped = strip_line_comment(raw_line).strip()
        if use_accumulator:
            use_accumulator.append(stripped)
            if ";" not in stripped:
                continue
            full = " ".join(use_accumulator)
            match = USE_RE.match(full)
            if match:
                module.uses.append(
                    UseDecl(
                        line=use_start_line,
                        raw=sanitize_signature(full),
                        visibility=compact_visibility(match.group("vis")),
                    )
                )
            use_accumulator = []
            attrs = []
            continue

        attr_match = ATTRIBUTE_RE.match(stripped)
        if attr_match:
            attrs.append(stripped)
            for feature in FEATURE_RE.findall(stripped):
                module.features.add(feature)
            continue

        if not stripped:
            continue

        if stripped.startswith("use ") or stripped.startswith("pub use "):
            if ";" in stripped:
                match = USE_RE.match(stripped)
                if match:
                    module.uses.append(
                        UseDecl(
                            line=line_no,
                            raw=sanitize_signature(stripped),
                            visibility=compact_visibility(match.group("vis")),
                        )
                    )
            else:
                use_start_line = line_no
                use_accumulator = [stripped]
            attrs = []
            continue

        mod_match = MOD_RE.match(stripped)
        if mod_match:
            path_attr = next((match for attr in attrs for match in PATH_ATTR_RE.findall(attr)), None)
            module.mods.append(
                ModDecl(
                    line=line_no,
                    name=mod_match.group("name"),
                    visibility=compact_visibility(mod_match.group("vis")),
                    path_attr=path_attr,
                    inline="{" in stripped,
                    cfgs=list(attrs),
                )
            )
            attrs = []
            continue

        include_match = INCLUDE_RE.match(stripped)
        if include_match:
            module.includes.append(IncludeDecl(line=line_no, path=include_match.group(1)))
            attrs = []
            continue

        macro_match = MACRO_RE.match(stripped)
        if macro_match:
            module.items.append(
                ItemDecl(
                    line=line_no,
                    kind="macro",
                    name=macro_match.group("name"),
                    visibility=compact_visibility(macro_match.group("vis")),
                    signature=sanitize_signature(stripped),
                    cfgs=list(attrs),
                    is_test=False,
                )
            )
            attrs = []
            continue

        item_match = ITEM_RE.match(stripped)
        if item_match:
            module.items.append(
                ItemDecl(
                    line=line_no,
                    kind=item_match.group("kind"),
                    name=item_match.group("name"),
                    visibility=compact_visibility(item_match.group("vis")),
                    signature=sanitize_signature(stripped),
                    cfgs=list(attrs),
                    is_test=any("#[test" in attr for attr in attrs),
                )
            )
            attrs = []
            continue

        impl_match = IMPL_RE.match(stripped)
        if impl_match:
            module.impls.append(
                ImplDecl(
                    line=line_no,
                    target=impl_match.group("target"),
                    trait=impl_match.group("trait"),
                    signature=sanitize_signature(stripped),
                    cfgs=list(attrs),
                )
            )
            attrs = []
            continue

        attrs = []


def candidate_module_file(parent_path: Path, child_name: str) -> list[Path]:
    if parent_path.name == "mod.rs":
        base = parent_path.parent
    else:
        base = parent_path.with_suffix("")
    return [base / f"{child_name}.rs", base / child_name / "mod.rs"]


def resolve_mod_decls(modules: dict[str, RustModule]) -> None:
    file_to_id = {
        module.source_path.resolve(): module.module_id
        for module in modules.values()
        if module.source_path is not None
    }
    for module in modules.values():
        if module.source_path is None:
            continue
        for decl in module.mods:
            if decl.inline:
                decl.target = f"{module.module_id}::{decl.name}"
                continue
            if decl.path_attr:
                target = file_to_id.get((module.source_path.parent / decl.path_attr).resolve())
                if target is not None:
                    decl.target = target
                    continue
            for candidate in candidate_module_file(module.source_path, decl.name):
                target = file_to_id.get(candidate.resolve())
                if target is not None:
                    decl.target = target
                    break


def resolve_include_decls(modules: dict[str, RustModule]) -> None:
    file_to_id = {
        module.source_path.resolve(): module.module_id
        for module in modules.values()
        if module.source_path is not None
    }
    for module in modules.values():
        if module.source_path is None:
            continue
        for include in module.includes:
            include.target = file_to_id.get((module.source_path.parent / include.path).resolve())
            if include.target and include.target != module.module_id:
                modules[include.target].incoming.append(module.module_id)


def add_synthetic_groups(crates: list[CrateInfo], modules: dict[str, RustModule]) -> None:
    for crate in crates:
        root_id = crate.module_name
        if root_id not in modules:
            modules[root_id] = RustModule(
                module_id=root_id,
                crate_name=crate.package,
                source_path=None,
                parent=None,
                kind="crate",
            )
    changed = True
    while changed:
        changed = False
        for module_id in list(modules):
            parent = parent_module(module_id)
            if parent is None or parent in modules:
                continue
            crate_prefix = module_id.split("::", 1)[0]
            crate_name = modules[module_id].crate_name
            modules[parent] = RustModule(
                module_id=parent,
                crate_name=crate_name,
                source_path=None,
                parent=parent_module(parent),
                kind="group" if parent != crate_prefix else "crate",
            )
            changed = True


def resolve_relative_target(current: str, use_path: str, known: set[str]) -> str | None:
    path = parse_use_path(use_path)
    if not path:
        return None
    crate_root = current.split("::", 1)[0]
    if path.startswith("crate::"):
        base = f"{crate_root}::{path[len('crate::'):]}"
    elif path == "crate":
        base = crate_root
    elif path.startswith("self::"):
        base = f"{current}::{path[len('self::'):]}"
    elif path == "self":
        base = current
    elif path.startswith("super::"):
        remaining = path
        base_parts = current.split("::")
        while remaining.startswith("super::"):
            remaining = remaining[len("super::") :]
            if len(base_parts) > 1:
                base_parts.pop()
        base = "::".join((*base_parts, remaining))
    else:
        first = path.split("::", 1)[0]
        if first in {target.split("::", 1)[0] for target in known}:
            base = path
        else:
            base = f"{current}::{path}"

    parts = base.split("::")
    for length in range(len(parts), 0, -1):
        candidate = "::".join(parts[:length])
        if candidate in known:
            return candidate
    return None


def resolve_imports(modules: dict[str, RustModule]) -> None:
    known = set(modules)
    for module in modules.values():
        for use_decl in module.uses:
            use_decl.target = resolve_relative_target(module.module_id, use_decl.raw, known)
            if use_decl.target and use_decl.target != module.module_id:
                modules[use_decl.target].incoming.append(module.module_id)


def wire_children(modules: dict[str, RustModule]) -> None:
    for module in modules.values():
        parent = module.parent
        if parent and parent in modules:
            modules[parent].children.append(module.module_id)
    for module in modules.values():
        module.children = sorted(set(module.children))
        module.incoming = sorted(set(module.incoming))


def crate_to_dict(crate: CrateInfo, root: Path) -> dict:
    return {
        "package": crate.package,
        "module_name": crate.module_name,
        "root": relpath(crate.root, root),
        "features": crate.features,
        "dependencies": crate.dependencies,
    }


def relpath(path: Path, root: Path) -> str:
    try:
        return path.resolve().relative_to(root).as_posix()
    except ValueError:
        return path.resolve().as_posix()


def module_to_dict(module: RustModule, root: Path, output_parent: Path) -> dict:
    source = None
    href = None
    if module.source_path is not None:
        source = relpath(module.source_path, root)
        href = Path(os.path.relpath(module.source_path, start=output_parent)).as_posix()
    return {
        "id": module.module_id,
        "crate": module.crate_name,
        "source": source,
        "href": href,
        "source_text": module.source_path.read_text(encoding="utf-8")
        if module.source_path is not None
        else None,
        "parent": module.parent,
        "kind": module.kind,
        "line_count": module.line_count,
        "loc": module.loc,
        "children": module.children,
        "incoming": module.incoming,
        "features": sorted(module.features),
        "uses": [use.__dict__ for use in module.uses],
        "mods": [decl.__dict__ for decl in module.mods],
        "includes": [decl.__dict__ for decl in module.includes],
        "items": [item.__dict__ for item in module.items],
        "impls": [impl.__dict__ for impl in module.impls],
    }


def load_workspace(root: Path, output: Path) -> dict:
    crates = discover_crates(root)
    modules: dict[str, RustModule] = {}
    for crate in crates:
        for source in rust_sources(crate):
            module_id = module_id_for_source(crate, source)
            modules[module_id] = RustModule(
                module_id=module_id,
                crate_name=crate.package,
                source_path=source,
                parent=parent_module(module_id),
            )
    add_synthetic_groups(crates, modules)
    for module in modules.values():
        parse_source(module)
    resolve_mod_decls(modules)
    resolve_include_decls(modules)
    resolve_imports(modules)
    wire_children(modules)

    source_modules = [module for module in modules.values() if module.source_path is not None]
    stats = {
        "crates": len(crates),
        "modules": len(modules),
        "source_files": len(source_modules),
        "lines": sum(module.line_count for module in source_modules),
        "loc": sum(module.loc for module in source_modules),
        "items": sum(len(module.items) for module in source_modules),
        "impls": sum(len(module.impls) for module in source_modules),
        "tests": sum(1 for module in source_modules for item in module.items if item.is_test),
    }
    return {
        "stats": stats,
        "crates": [crate_to_dict(crate, root) for crate in crates],
        "modules": {
            module_id: module_to_dict(module, root, output.parent.resolve())
            for module_id, module in sorted(modules.items())
        },
        "roots": sorted(crate.module_name for crate in crates),
    }


def load_profile(path: Path | None, root: Path) -> dict | None:
    if path is None:
        return None
    profile_path = path if path.is_absolute() else root / path
    return json.loads(profile_path.read_text(encoding="utf-8"))


def build_html(data: dict, title: str) -> str:
    data_json = json.dumps(data, separators=(",", ":")).replace("</", "<\\/")
    title_html = html.escape(title)
    return f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title_html}</title>
  <style>
    :root {{
      color-scheme: light;
      --bg: #f6f7f9;
      --panel: #ffffff;
      --text: #1f252e;
      --muted: #667085;
      --line: #d8dee8;
      --accent: #1463a5;
      --accent-soft: #e8f2fb;
      --green: #256f4a;
      --yellow: #805b00;
      --code: #f1f4f8;
      --shadow: 0 1px 3px rgba(20, 33, 61, 0.12);
    }}
    * {{ box-sizing: border-box; }}
    body {{
      margin: 0;
      font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      background: var(--bg);
      color: var(--text);
      letter-spacing: 0;
    }}
    header {{
      min-height: 56px;
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 16px;
      padding: 10px 18px;
      border-bottom: 1px solid var(--line);
      background: #fff;
      position: sticky;
      top: 0;
      z-index: 4;
    }}
    h1 {{
      margin: 0;
      font-size: 18px;
      font-weight: 700;
    }}
    h2 {{
      margin: 0 0 10px;
      font-size: 15px;
    }}
    h3 {{
      margin: 18px 0 8px;
      font-size: 13px;
      color: #394150;
      text-transform: uppercase;
      letter-spacing: 0;
    }}
    a {{ color: var(--accent); text-decoration: none; }}
    a:hover {{ text-decoration: underline; }}
    .meta {{
      color: var(--muted);
      font-size: 12px;
      white-space: nowrap;
    }}
    .layout {{
      display: grid;
      grid-template-columns: minmax(280px, 360px) minmax(0, 1fr);
      min-height: calc(100vh - 57px);
    }}
    aside {{
      border-right: 1px solid var(--line);
      background: #fff;
      overflow: auto;
      max-height: calc(100vh - 57px);
      position: sticky;
      top: 57px;
    }}
    main {{
      min-width: 0;
      padding: 18px;
    }}
    .search-wrap {{
      padding: 14px;
      border-bottom: 1px solid var(--line);
      display: grid;
      gap: 10px;
    }}
    input[type="search"] {{
      width: 100%;
      min-height: 36px;
      border: 1px solid #c9d2df;
      border-radius: 6px;
      padding: 8px 10px;
      font-size: 14px;
      color: var(--text);
      background: #fff;
    }}
    .tabs {{
      display: flex;
      gap: 6px;
      flex-wrap: wrap;
    }}
    .tab, .module-button {{
      border: 1px solid var(--line);
      background: #fff;
      color: var(--text);
      border-radius: 6px;
      cursor: pointer;
      font: inherit;
    }}
    .tab {{
      padding: 5px 8px;
      font-size: 12px;
    }}
    .tab.selected {{
      border-color: var(--accent);
      background: var(--accent-soft);
      color: #0a4d85;
    }}
    .module-list {{
      padding: 8px;
      display: grid;
      gap: 2px;
    }}
    .module-button {{
      width: 100%;
      min-height: 30px;
      text-align: left;
      padding: 5px 8px;
      display: grid;
      grid-template-columns: minmax(0, 1fr) auto;
      gap: 8px;
      align-items: center;
    }}
    .module-button:hover {{ background: #f2f6fb; }}
    .module-button.selected {{
      border-color: var(--accent);
      background: var(--accent-soft);
    }}
    .module-name {{
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
      font-size: 12px;
    }}
    .module-count {{
      color: var(--muted);
      font-size: 11px;
    }}
    .topline {{
      display: flex;
      align-items: flex-start;
      justify-content: space-between;
      gap: 16px;
      margin-bottom: 14px;
    }}
    .module-title {{
      margin: 0;
      font-size: 22px;
      line-height: 1.25;
      overflow-wrap: anywhere;
      font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
    }}
    .panel {{
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 8px;
      box-shadow: var(--shadow);
      padding: 14px;
      margin-bottom: 14px;
    }}
    .stats {{
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
      gap: 10px;
    }}
    .stat {{
      border: 1px solid var(--line);
      border-radius: 6px;
      padding: 8px;
      background: #fbfcfe;
    }}
    .stat strong {{
      display: block;
      font-size: 18px;
    }}
    .stat span {{
      color: var(--muted);
      font-size: 12px;
    }}
    .grid {{
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
      gap: 14px;
    }}
    .pill-row {{
      display: flex;
      flex-wrap: wrap;
      gap: 6px;
    }}
    .pill {{
      display: inline-flex;
      align-items: center;
      max-width: 100%;
      min-height: 24px;
      padding: 3px 7px;
      border: 1px solid #cbd5e1;
      border-radius: 6px;
      background: #f8fafc;
      color: #344054;
      font-size: 12px;
      overflow-wrap: anywhere;
    }}
    .pill.public {{ border-color: #a8d7bd; background: #eff9f3; color: var(--green); }}
    .pill.feature {{ border-color: #f2d27a; background: #fff7df; color: var(--yellow); }}
    .hotspot-badge {{
      display: inline-flex;
      align-items: center;
      min-height: 20px;
      padding: 1px 6px;
      border-radius: 4px;
      background: #fff1ee;
      color: #9f2d20;
      border: 1px solid #f1b4aa;
      font-size: 11px;
      white-space: nowrap;
    }}
    .hotspot-meter {{
      width: 100%;
      min-width: 72px;
      height: 8px;
      border-radius: 999px;
      overflow: hidden;
      background: #edf1f6;
      border: 1px solid #d8dee8;
    }}
    .hotspot-fill {{
      height: 100%;
      background: #d94736;
    }}
    .empty {{
      color: var(--muted);
      font-size: 13px;
      padding: 8px 0;
    }}
    table {{
      width: 100%;
      border-collapse: collapse;
      table-layout: fixed;
    }}
    th, td {{
      border-bottom: 1px solid var(--line);
      padding: 7px 6px;
      text-align: left;
      vertical-align: top;
      font-size: 13px;
    }}
    th {{
      color: var(--muted);
      font-weight: 600;
      background: #fbfcfe;
    }}
    td.code, code {{
      font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
      font-size: 12px;
    }}
    code {{
      background: var(--code);
      border: 1px solid #dde4ee;
      border-radius: 4px;
      padding: 1px 4px;
    }}
    .signature {{
      font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
      font-size: 12px;
      overflow-wrap: anywhere;
    }}
    .source-panel {{
      padding: 0;
      overflow: hidden;
    }}
    .source-header {{
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      min-height: 44px;
      padding: 10px 14px;
      border-bottom: 1px solid var(--line);
      background: #fbfcfe;
    }}
    .source-header h2 {{
      margin: 0;
    }}
    .source-view {{
      max-height: 58vh;
      overflow: auto;
      background: #0f1720;
      color: #d8dee9;
      font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
      font-size: 12px;
      line-height: 1.55;
    }}
    .source-row {{
      display: grid;
      grid-template-columns: 58px minmax(max-content, 1fr);
      min-height: 20px;
    }}
    .source-row:target {{
      background: rgba(83, 142, 221, 0.22);
    }}
    .source-row.focused {{
      background: rgba(83, 142, 221, 0.22);
    }}
    .source-row:hover {{
      background: rgba(255, 255, 255, 0.05);
    }}
    .source-line-number {{
      position: sticky;
      left: 0;
      z-index: 1;
      padding: 0 10px 0 8px;
      border-right: 1px solid #263241;
      background: #111c29;
      color: #7d8ea3;
      text-align: right;
      user-select: none;
    }}
    .source-line-number a {{
      color: inherit;
      text-decoration: none;
    }}
    .source-code {{
      white-space: pre;
      padding: 0 14px 0 10px;
      tab-size: 4;
    }}
    .tok-kw {{ color: #8ec5ff; font-weight: 600; }}
    .tok-type {{ color: #79d6c9; }}
    .tok-lit {{ color: #e9c46a; }}
    .tok-num {{ color: #f4a261; }}
    .tok-str {{ color: #a7d37c; }}
    .tok-comment {{ color: #8b99aa; font-style: italic; }}
    .tok-attr {{ color: #c2a8ff; }}
    .graph {{
      width: 100%;
      min-height: 260px;
      border: 1px solid var(--line);
      border-radius: 6px;
      background: #fbfcfe;
    }}
    .graph text {{
      font-family: ui-sans-serif, system-ui, sans-serif;
      font-size: 12px;
      fill: #1f252e;
    }}
    .graph .edge {{ stroke: #9aa8ba; stroke-width: 1.3; marker-end: url(#arrow); }}
    .graph .node {{ fill: #fff; stroke: #b7c2d0; stroke-width: 1.2; cursor: pointer; }}
    .graph .selected-node {{ fill: var(--accent-soft); stroke: var(--accent); stroke-width: 1.8; }}
    .graph .label {{ pointer-events: none; }}
    @media (max-width: 860px) {{
      header {{ position: static; align-items: flex-start; flex-direction: column; }}
      .layout {{ grid-template-columns: 1fr; }}
      aside {{ position: static; max-height: 45vh; border-right: 0; border-bottom: 1px solid var(--line); }}
      main {{ padding: 12px; }}
      .topline {{ display: block; }}
      .meta {{ white-space: normal; }}
    }}
  </style>
</head>
<body>
  <header>
    <h1>{title_html}</h1>
    <div class="meta" id="headerMeta"></div>
  </header>
  <div class="layout">
    <aside>
      <div class="search-wrap">
        <input id="search" type="search" placeholder="Search modules, files, symbols, imports">
        <div class="tabs" id="crateTabs"></div>
      </div>
      <div class="module-list" id="moduleList"></div>
    </aside>
    <main id="content"></main>
  </div>
  <script id="workspace-data" type="application/json">{data_json}</script>
  <script>
    const DATA = JSON.parse(document.getElementById("workspace-data").textContent);
    const PROFILE = DATA.profile || null;
    const MODULES = DATA.modules;
    const MODULE_IDS = Object.keys(MODULES).sort();
    const GLOBAL_PROFILE_MAX = PROFILE
      ? Math.max(1, ...Object.values(PROFILE.modules || {{}}).map((entry) => entry.inclusive_ns || 0))
      : 1;
    let selected = decodeURIComponent(location.hash.slice(1)) || DATA.roots[0];
    if (!MODULES[selected]) selected = DATA.roots[0];
    let search = "";
    let crateFilter = "all";
    let focusedSourceLine = null;
    const RUST_KEYWORDS = new Set([
      "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else",
      "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop",
      "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self",
      "static", "struct", "super", "trait", "true", "type", "unsafe", "use",
      "where", "while"
    ]);
    const RUST_TYPES = new Set([
      "bool", "char", "f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize",
      "str", "u8", "u16", "u32", "u64", "u128", "usize"
    ]);
    const RUST_LITERALS = new Set(["None", "Some", "Ok", "Err"]);

    function esc(value) {{
      return String(value ?? "").replace(/[&<>"']/g, (ch) => ({{
        "&": "&amp;",
        "<": "&lt;",
        ">": "&gt;",
        '"': "&quot;",
        "'": "&#039;"
      }}[ch]));
    }}

    function hashFor(id) {{
      return "#" + encodeURIComponent(id);
    }}

    function selectModule(id) {{
      if (!MODULES[id]) return;
      selected = id;
      history.pushState({{ id }}, "", hashFor(id));
      renderAll();
    }}

    function searchableText(mod) {{
      const itemText = mod.items.map((item) => item.name + " " + item.signature).join(" ");
      const uses = mod.uses.map((entry) => entry.raw).join(" ");
      const includes = mod.includes.map((entry) => entry.path).join(" ");
      return `${{mod.id}} ${{mod.source || ""}} ${{itemText}} ${{uses}} ${{includes}}`.toLowerCase();
    }}

    function renderCrateTabs() {{
      const crates = ["all", ...DATA.crates.map((crate) => crate.package)];
      document.getElementById("crateTabs").innerHTML = crates.map((name) => `
        <button class="tab ${{name === crateFilter ? "selected" : ""}}" type="button" onclick="crateFilter='${{esc(name)}}'; renderAll();">
          ${{esc(name === "all" ? "All" : name)}}
        </button>
      `).join("");
    }}

    function renderModuleList() {{
      const query = search.trim().toLowerCase();
      const modules = MODULE_IDS
        .map((id) => MODULES[id])
        .filter((mod) => crateFilter === "all" || mod.crate === crateFilter)
        .filter((mod) => !query || searchableText(mod).includes(query));
      document.getElementById("moduleList").innerHTML = modules.map((mod) => {{
        const depth = Math.max(0, mod.id.split("::").length - 1);
        const label = mod.id;
        const heat = profileModule(mod.id);
        const counts = heat ? formatTime(heat.inclusive_ns || 0) : `${{mod.items.length}} items`;
        const style = heat ? heatStyle(heat.inclusive_ns || 0, GLOBAL_PROFILE_MAX) : "";
        return `
          <button class="module-button ${{mod.id === selected ? "selected" : ""}}" type="button" style="${{style}}" onclick="selectModule('${{esc(mod.id)}}')">
            <span class="module-name" style="padding-left: ${{Math.min(depth, 7) * 12}}px">${{esc(label)}}</span>
            <span class="module-count">${{esc(counts)}}</span>
          </button>
        `;
      }}).join("") || '<div class="empty">No matching modules.</div>';
    }}

    function lineLink(mod, line) {{
      if (!mod.source_text) return esc(String(line));
      return `<a href="#source-line-${{line}}" onclick="scrollSourceLine(${{line}}); return false;">${{line}}</a>`;
    }}

    function moduleLink(id, label = id) {{
      if (!id || !MODULES[id]) return esc(label || "");
      return `<a href="${{hashFor(id)}}" onclick="selectModule('${{esc(id)}}'); return false;">${{esc(label)}}</a>`;
    }}

    function profileModule(id) {{
      return PROFILE && PROFILE.modules ? PROFILE.modules[id] : null;
    }}

    function profileItem(moduleId, itemName) {{
      const key = `${{moduleId}}::${{itemName}}`;
      return PROFILE && PROFILE.items ? PROFILE.items[key] : null;
    }}

    function heatStyle(nanos, maxNanos) {{
      if (!nanos || !maxNanos) return "";
      const ratio = Math.max(0, Math.min(1, nanos / maxNanos));
      const alpha = 0.10 + Math.sqrt(ratio) * 0.42;
      const stop = Math.round(8 + ratio * 92);
      return `background: linear-gradient(90deg, rgba(217, 71, 54, ${{alpha.toFixed(3)}}) 0%, rgba(217, 71, 54, ${{alpha.toFixed(3)}}) ${{stop}}%, transparent ${{stop}}%); border-color: rgba(217, 71, 54, ${{Math.min(0.75, alpha + 0.15).toFixed(3)}});`;
    }}

    function formatTime(nanos) {{
      if (!nanos) return "0 us";
      if (nanos >= 1_000_000_000) return `${{(nanos / 1_000_000_000).toFixed(2)}} s`;
      if (nanos >= 1_000_000) return `${{(nanos / 1_000_000).toFixed(2)}} ms`;
      if (nanos >= 1_000) return `${{(nanos / 1_000).toFixed(1)}} us`;
      return `${{nanos}} ns`;
    }}

    function hotspotBadge(nanos, maxNanos = GLOBAL_PROFILE_MAX) {{
      if (!nanos) return "";
      const pct = maxNanos ? (nanos / maxNanos) * 100 : 0;
      return `<span class="hotspot-badge">${{formatTime(nanos)}} | ${{pct.toFixed(1)}}%</span>`;
    }}

    function hotspotMeter(nanos, maxNanos) {{
      if (!nanos || !maxNanos) return "";
      const pct = Math.max(0, Math.min(100, (nanos / maxNanos) * 100));
      return `<div class="hotspot-meter"><div class="hotspot-fill" style="width:${{pct.toFixed(1)}}%"></div></div>`;
    }}

    function scrollSourceLine(line) {{
      focusedSourceLine = line;
      const element = document.getElementById(`source-line-${{line}}`);
      if (element) {{
        element.scrollIntoView({{ block: "center", inline: "nearest" }});
        renderSourceFocus();
      }}
    }}

    function renderSourceFocus() {{
      document.querySelectorAll(".source-row.focused").forEach((row) => row.classList.remove("focused"));
      if (focusedSourceLine === null) return;
      const element = document.getElementById(`source-line-${{focusedSourceLine}}`);
      if (element) element.classList.add("focused");
    }}

    function tokenSpan(className, text) {{
      return `<span class="${{className}}">${{esc(text)}}</span>`;
    }}

    function isIdentStart(ch) {{
      return /[A-Za-z_]/.test(ch);
    }}

    function isIdentChar(ch) {{
      return /[A-Za-z0-9_]/.test(ch);
    }}

    function rawStringEndMarker(line, index) {{
      if (line[index] !== "r") return null;
      let cursor = index + 1;
      while (line[cursor] === "#") cursor += 1;
      if (line[cursor] !== '"') return null;
      return '"' + "#".repeat(cursor - index - 1);
    }}

    function highlightCodeSegment(line, state) {{
      let out = "";
      let index = 0;
      while (index < line.length) {{
        if (state.blockComment) {{
          const end = line.indexOf("*/", index);
          if (end === -1) {{
            out += tokenSpan("tok-comment", line.slice(index));
            index = line.length;
            continue;
          }}
          out += tokenSpan("tok-comment", line.slice(index, end + 2));
          index = end + 2;
          state.blockComment = false;
          continue;
        }}

        if (line.startsWith("/*", index)) {{
          const end = line.indexOf("*/", index + 2);
          if (end === -1) {{
            out += tokenSpan("tok-comment", line.slice(index));
            state.blockComment = true;
            break;
          }}
          out += tokenSpan("tok-comment", line.slice(index, end + 2));
          index = end + 2;
          continue;
        }}

        if (line.startsWith("//", index)) {{
          out += tokenSpan("tok-comment", line.slice(index));
          break;
        }}

        const rawEnd = rawStringEndMarker(line, index);
        if (rawEnd) {{
          const stringStart = line.indexOf('"', index);
          const end = line.indexOf(rawEnd, stringStart + 1);
          if (end === -1) {{
            out += tokenSpan("tok-str", line.slice(index));
            break;
          }}
          out += tokenSpan("tok-str", line.slice(index, end + rawEnd.length));
          index = end + rawEnd.length;
          continue;
        }}

        if (line[index] === '"') {{
          let end = index + 1;
          while (end < line.length) {{
            if (line[end] === "\\\\") {{
              end += 2;
              continue;
            }}
            if (line[end] === '"') {{
              end += 1;
              break;
            }}
            end += 1;
          }}
          out += tokenSpan("tok-str", line.slice(index, end));
          index = end;
          continue;
        }}

        if (line[index] === "'" && index + 1 < line.length && !isIdentStart(line[index + 1])) {{
          let end = index + 1;
          while (end < line.length) {{
            if (line[end] === "\\\\") {{
              end += 2;
              continue;
            }}
            if (line[end] === "'") {{
              end += 1;
              break;
            }}
            end += 1;
          }}
          out += tokenSpan("tok-str", line.slice(index, end));
          index = end;
          continue;
        }}

        const ch = line[index];
        if (isIdentStart(ch)) {{
          let end = index + 1;
          while (end < line.length && isIdentChar(line[end])) end += 1;
          const ident = line.slice(index, end);
          if (RUST_KEYWORDS.has(ident)) {{
            out += tokenSpan("tok-kw", ident);
          }} else if (RUST_TYPES.has(ident)) {{
            out += tokenSpan("tok-type", ident);
          }} else if (RUST_LITERALS.has(ident)) {{
            out += tokenSpan("tok-lit", ident);
          }} else {{
            out += esc(ident);
          }}
          index = end;
          continue;
        }}

        if (/[0-9]/.test(ch)) {{
          let end = index + 1;
          while (end < line.length && /[A-Za-z0-9_.]/.test(line[end])) end += 1;
          out += tokenSpan("tok-num", line.slice(index, end));
          index = end;
          continue;
        }}

        out += esc(ch);
        index += 1;
      }}
      return out;
    }}

    function highlightRustLine(line, state) {{
      if (!state.blockComment && line.trimStart().startsWith("#[")) {{
        return tokenSpan("tok-attr", line);
      }}
      return highlightCodeSegment(line, state);
    }}

    function renderPills(values, className = "") {{
      if (!values.length) return '<div class="empty">None.</div>';
      return `<div class="pill-row">${{values.map((value) => `<span class="pill ${{className}}">${{esc(value)}}</span>`).join("")}}</div>`;
    }}

    function renderModulePills(ids) {{
      if (!ids.length) return '<div class="empty">None.</div>';
      const maxNanos = Math.max(1, ...ids.map((id) => (profileModule(id) || {{}}).inclusive_ns || 0));
      return `<div class="pill-row">${{ids.map((id) => {{
        const heat = profileModule(id);
        const nanos = heat ? heat.inclusive_ns || 0 : 0;
        const style = heatStyle(nanos, maxNanos);
        return `<span class="pill" style="${{style}}">${{moduleLink(id)}}${{nanos ? " " + hotspotBadge(nanos, maxNanos) : ""}}</span>`;
      }}).join("")}}</div>`;
    }}

    function itemRows(items) {{
      if (!items.length) return '<tr><td colspan="5" class="empty">None.</td></tr>';
      const maxNanos = Math.max(1, ...items.map((item) => (profileItem(selected, item.name) || {{}}).nanos || 0));
      return items.map((item) => {{
        const heat = profileItem(selected, item.name);
        const nanos = heat ? heat.nanos || 0 : 0;
        return `
        <tr style="${{heatStyle(nanos, maxNanos)}}">
          <td>${{lineLink(MODULES[selected], item.line)}}</td>
          <td><code>${{esc(item.kind)}}</code></td>
          <td class="code">${{esc(item.name)}}</td>
          <td>${{esc(item.visibility)}}</td>
          <td><div class="signature">${{esc(item.signature)}} ${{nanos ? hotspotBadge(nanos, maxNanos) : ""}}</div>${{item.cfgs.length ? `<div>${{renderPills(item.cfgs, "feature")}}</div>` : ""}}</td>
        </tr>
      `;
      }}).join("");
    }}

    function implRows(impls) {{
      if (!impls.length) return '<tr><td colspan="4" class="empty">None.</td></tr>';
      return impls.map((entry) => `
        <tr>
          <td>${{lineLink(MODULES[selected], entry.line)}}</td>
          <td class="code">${{esc(entry.target)}}</td>
          <td>${{entry.trait ? esc(entry.trait) : "inherent"}}</td>
          <td><div class="signature">${{esc(entry.signature)}}</div>${{entry.cfgs.length ? `<div>${{renderPills(entry.cfgs, "feature")}}</div>` : ""}}</td>
        </tr>
      `).join("");
    }}

    function useRows(uses) {{
      if (!uses.length) return '<tr><td colspan="4" class="empty">None.</td></tr>';
      return uses.map((entry) => `
        <tr>
          <td>${{lineLink(MODULES[selected], entry.line)}}</td>
          <td>${{esc(entry.visibility)}}</td>
          <td>${{entry.target ? moduleLink(entry.target) : ""}}</td>
          <td><div class="signature">${{esc(entry.raw)}}</div></td>
        </tr>
      `).join("");
    }}

    function modRows(mods) {{
      if (!mods.length) return '<tr><td colspan="5" class="empty">None.</td></tr>';
      return mods.map((entry) => `
        <tr>
          <td>${{lineLink(MODULES[selected], entry.line)}}</td>
          <td class="code">${{esc(entry.name)}}</td>
          <td>${{esc(entry.visibility)}}</td>
          <td>${{entry.target ? moduleLink(entry.target) : (entry.inline ? "inline" : "unresolved")}}${{entry.path_attr ? `<div class="meta">#[path = "${{esc(entry.path_attr)}}"]</div>` : ""}}</td>
          <td>${{entry.cfgs.length ? renderPills(entry.cfgs, "feature") : ""}}</td>
        </tr>
      `).join("");
    }}

    function includeRows(includes) {{
      if (!includes.length) return '<tr><td colspan="3" class="empty">None.</td></tr>';
      return includes.map((entry) => `
        <tr>
          <td>${{lineLink(MODULES[selected], entry.line)}}</td>
          <td class="code">${{esc(entry.path)}}</td>
          <td>${{entry.target ? moduleLink(entry.target) : "unresolved"}}</td>
        </tr>
      `).join("");
    }}

    function graphNeighbors(mod) {{
      const targets = mod.uses.map((entry) => entry.target).filter((id) => id && id !== mod.id);
      const included = mod.includes.map((entry) => entry.target).filter((id) => id && id !== mod.id);
      const neighbors = [...new Set([...mod.children, ...included, ...targets, ...mod.incoming])].filter((id) => MODULES[id]);
      return neighbors.slice(0, 22);
    }}

    function renderGraph(mod) {{
      const neighbors = graphNeighbors(mod);
      if (!neighbors.length) return '<div class="empty">No local module edges for this node.</div>';
      const width = 860;
      const height = 310;
      const cx = width / 2;
      const cy = height / 2;
      const rx = 330;
      const ry = 110;
      const centerW = 230;
      const centerH = 42;
      const nodes = neighbors.map((id, index) => {{
        const angle = (Math.PI * 2 * index) / neighbors.length - Math.PI / 2;
        return {{
          id,
          x: cx + Math.cos(angle) * rx,
          y: cy + Math.sin(angle) * ry
        }};
      }});
      const edges = nodes.map((node) => {{
        const included = mod.includes.some((entry) => entry.target === node.id);
        const relation = mod.children.includes(node.id) || included ? "child" : mod.incoming.includes(node.id) ? "incoming" : "import";
        const x1 = relation === "incoming" ? node.x : cx;
        const y1 = relation === "incoming" ? node.y : cy;
        const x2 = relation === "incoming" ? cx : node.x;
        const y2 = relation === "incoming" ? cy : node.y;
        return `<line class="edge" x1="${{x1}}" y1="${{y1}}" x2="${{x2}}" y2="${{y2}}"></line>`;
      }}).join("");
      const renderedNodes = nodes.map((node) => {{
        const label = node.id.split("::").slice(-2).join("::");
        const heat = profileModule(node.id);
        const maxNanos = Math.max(1, ...nodes.map((candidate) => (profileModule(candidate.id) || {{}}).inclusive_ns || 0));
        const intensity = heat ? Math.max(0.12, Math.min(0.78, Math.sqrt((heat.inclusive_ns || 0) / maxNanos) * 0.78)) : 0;
        const fill = heat ? `rgba(217, 71, 54, ${{intensity.toFixed(3)}})` : "#fff";
        return `
          <g onclick="selectModule('${{esc(node.id)}}')">
            <rect class="node" x="${{node.x - 95}}" y="${{node.y - 17}}" width="190" height="34" rx="6" style="fill:${{fill}}"></rect>
            <text class="label" x="${{node.x}}" y="${{node.y + 4}}" text-anchor="middle">${{esc(label.length > 24 ? label.slice(0, 21) + "..." : label)}}</text>
          </g>
        `;
      }}).join("");
      return `
        <svg class="graph" viewBox="0 0 ${{width}} ${{height}}" role="img" aria-label="Local dependency graph">
          <defs>
            <marker id="arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
              <path d="M0,0 L8,4 L0,8 z" fill="#9aa8ba"></path>
            </marker>
          </defs>
          ${{edges}}
          <rect class="node selected-node" x="${{cx - centerW / 2}}" y="${{cy - centerH / 2}}" width="${{centerW}}" height="${{centerH}}" rx="6"></rect>
          <text class="label" x="${{cx}}" y="${{cy + 4}}" text-anchor="middle">${{esc(mod.id.split("::").slice(-2).join("::"))}}</text>
          ${{renderedNodes}}
        </svg>
      `;
    }}

    function renderProfileSummary(mod) {{
      if (!PROFILE) return "";
      const moduleProfile = profileModule(mod.id);
      if (!moduleProfile) {{
        return `
          <section class="panel">
            <h2>Hotspots</h2>
            <div class="empty">No wall-time buckets mapped to this module in the loaded profile.</div>
          </section>
        `;
      }}
      const stages = moduleProfile.stages || [];
      const maxStage = Math.max(1, ...stages.map((stage) => stage.nanos || 0));
      const stageRows = stages.slice(0, 12).map((stage) => `
        <tr style="${{heatStyle(stage.nanos || 0, maxStage)}}">
          <td class="code">${{esc(stage.name)}}</td>
          <td>${{formatTime(stage.nanos || 0)}}</td>
          <td>${{hotspotMeter(stage.nanos || 0, maxStage)}}</td>
        </tr>
      `).join("") || '<tr><td colspan="3" class="empty">None.</td></tr>';
      return `
        <section class="panel">
          <h2>Hotspots</h2>
          <div class="stats" style="margin-bottom: 12px;">
            <div class="stat"><strong>${{formatTime(moduleProfile.inclusive_ns || 0)}}</strong><span>inclusive wall time</span></div>
            <div class="stat"><strong>${{formatTime(moduleProfile.direct_ns || 0)}}</strong><span>direct mapped wall time</span></div>
            <div class="stat"><strong>${{((moduleProfile.share || 0) * 100).toFixed(2)}}%</strong><span>profile share</span></div>
          </div>
          <table>
            <thead><tr><th>Bucket</th><th style="width:130px">Time</th><th style="width:180px">Relative</th></tr></thead>
            <tbody>${{stageRows}}</tbody>
          </table>
          <div class="meta" style="margin-top: 8px;">${{esc(PROFILE.note || "")}}</div>
        </section>
      `;
    }}

    function renderSourceCode(mod) {{
      if (!mod.source_text) {{
        return `
          <section class="panel source-panel">
            <div class="source-header">
              <h2>Source</h2>
              <span class="meta">synthetic grouping node</span>
            </div>
            <div class="empty" style="padding: 14px;">No source file for this node.</div>
          </section>
        `;
      }}
      const state = {{ blockComment: false }};
      const lines = mod.source_text.replace(/\\r\\n/g, "\\n").replace(/\\r/g, "\\n").split("\\n");
      const lineHeat = new Map();
      for (const item of mod.items) {{
        const heat = profileItem(mod.id, item.name);
        if (heat && heat.nanos) lineHeat.set(item.line, Math.max(lineHeat.get(item.line) || 0, heat.nanos));
      }}
      const maxLineNanos = Math.max(1, ...lineHeat.values());
      const rendered = lines.map((line, index) => {{
        const lineNumber = index + 1;
        const nanos = lineHeat.get(lineNumber) || 0;
        return `
          <div class="source-row" id="source-line-${{lineNumber}}" style="${{heatStyle(nanos, maxLineNanos)}}">
            <div class="source-line-number"><a href="#source-line-${{lineNumber}}" onclick="scrollSourceLine(${{lineNumber}}); return false;">${{lineNumber}}</a></div>
            <div class="source-code">${{highlightRustLine(line, state) || " "}}</div>
          </div>
        `;
      }}).join("");
      return `
        <section class="panel source-panel">
          <div class="source-header">
            <h2>Source</h2>
            <span class="meta">${{esc(mod.source)}} | ${{lines.length}} lines</span>
          </div>
          <div class="source-view">${{rendered}}</div>
        </section>
      `;
    }}

    function renderContent() {{
      const mod = MODULES[selected];
      focusedSourceLine = null;
      const tests = mod.items.filter((item) => item.is_test);
      const publicItems = mod.items.filter((item) => item.visibility.startsWith("pub"));
      document.getElementById("content").innerHTML = `
        <div class="topline">
          <div>
            <div class="meta">${{esc(mod.crate)}}${{mod.kind !== "source" ? " | " + esc(mod.kind) : ""}}</div>
            <h2 class="module-title">${{esc(mod.id)}}</h2>
            <div class="meta">${{mod.source ? `<a href="${{esc(mod.href)}}">${{esc(mod.source)}}</a>` : "synthetic grouping node"}}</div>
          </div>
          <div class="meta">${{mod.parent ? "parent: " + moduleLink(mod.parent) : "crate root"}}</div>
        </div>
        <section class="panel stats">
          <div class="stat"><strong>${{mod.line_count}}</strong><span>lines</span></div>
          <div class="stat"><strong>${{mod.loc}}</strong><span>source loc</span></div>
          <div class="stat"><strong>${{mod.items.length}}</strong><span>items</span></div>
          <div class="stat"><strong>${{publicItems.length}}</strong><span>public items</span></div>
          <div class="stat"><strong>${{mod.impls.length}}</strong><span>impl blocks</span></div>
          <div class="stat"><strong>${{tests.length}}</strong><span>tests</span></div>
        </section>
        ${{renderProfileSummary(mod)}}
        <section class="grid">
          <div class="panel">
            <h2>Local Graph</h2>
            ${{renderGraph(mod)}}
          </div>
          <div class="panel">
            <h2>Module Edges</h2>
            <h3>Children</h3>
            ${{renderModulePills(mod.children)}}
            <h3>Included Files</h3>
            ${{renderModulePills(mod.includes.map((entry) => entry.target).filter((id) => id))}}
            <h3>Imported By</h3>
            ${{renderModulePills(mod.incoming)}}
            <h3>Feature Gates</h3>
            ${{renderPills(mod.features, "feature")}}
          </div>
        </section>
        <section class="panel">
          <h2>Module Declarations</h2>
          <table>
            <thead><tr><th style="width:70px">Line</th><th>Name</th><th>Visibility</th><th>Target</th><th>Cfg</th></tr></thead>
            <tbody>${{modRows(mod.mods)}}</tbody>
          </table>
        </section>
        <section class="panel">
          <h2>Includes</h2>
          <table>
            <thead><tr><th style="width:70px">Line</th><th>Path</th><th>Target</th></tr></thead>
            <tbody>${{includeRows(mod.includes)}}</tbody>
          </table>
        </section>
        <section class="panel">
          <h2>Imports</h2>
          <table>
            <thead><tr><th style="width:70px">Line</th><th style="width:110px">Visibility</th><th>Local Target</th><th>Statement</th></tr></thead>
            <tbody>${{useRows(mod.uses)}}</tbody>
          </table>
        </section>
        <section class="panel">
          <h2>Items</h2>
          <table>
            <thead><tr><th style="width:70px">Line</th><th style="width:80px">Kind</th><th>Name</th><th style="width:110px">Visibility</th><th>Signature</th></tr></thead>
            <tbody>${{itemRows(mod.items)}}</tbody>
          </table>
        </section>
        <section class="panel">
          <h2>Impl Blocks</h2>
          <table>
            <thead><tr><th style="width:70px">Line</th><th>Target</th><th>Trait</th><th>Signature</th></tr></thead>
            <tbody>${{implRows(mod.impls)}}</tbody>
          </table>
        </section>
        ${{renderSourceCode(mod)}}
      `;
    }}

    function renderAll() {{
      renderCrateTabs();
      renderModuleList();
      renderContent();
    }}

    document.getElementById("search").addEventListener("input", (event) => {{
      search = event.target.value;
      renderModuleList();
    }});

    window.addEventListener("popstate", () => {{
      const id = decodeURIComponent(location.hash.slice(1));
      if (MODULES[id]) {{
        selected = id;
        renderAll();
      }}
    }});

    document.getElementById("headerMeta").textContent =
      `${{DATA.stats.crates}} crates | ${{DATA.stats.source_files}} Rust files | ${{DATA.stats.lines}} lines | ${{DATA.stats.tests}} tests`;
    renderAll();
  </script>
</body>
</html>
"""


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path("."), help="workspace root")
    parser.add_argument("--output", type=Path, required=True, help="HTML output path")
    parser.add_argument(
        "--profile-json",
        type=Path,
        help="optional hotspot profile JSON emitted by scripts/summarize_hotspots.py",
    )
    parser.add_argument(
        "--title",
        default="FrameFinery Engine Code Browser",
        help="HTML page title",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    root = args.root.resolve()
    output = args.output
    if not output.is_absolute():
        output = root / output
    output = output.resolve()
    data = load_workspace(root, output)
    data["profile"] = load_profile(args.profile_json, root)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(build_html(data, args.title), encoding="utf-8")
    stats = data["stats"]
    print(
        "Rust code browser written to "
        f"{output.relative_to(root)} "
        f"({stats['source_files']} files, {stats['modules']} modules, {stats['lines']} lines)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
