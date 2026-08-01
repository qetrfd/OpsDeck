#!/usr/bin/env python3

from __future__ import annotations

import re
import shutil
import sys
import tempfile
from datetime import datetime
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "src"


REPLACEMENTS = {
    "lib.rs": {
        "config_path": '''pub fn config_path() -> Result<PathBuf, String> {
    crate::paths::data_file("projects.json")
}''',
    },

    "history.rs": {
        "history_path": '''pub fn history_path() -> Result<PathBuf, String> {
    crate::paths::data_file("history.json")
}''',
    },

    "gate.rs": {
        "policy_path": '''pub fn policy_path() -> Result<PathBuf, String> {
    crate::paths::data_file("policies.json")
}''',

        "default_gate_path": '''pub fn default_gate_path(
    project_name: &str,
) -> Result<PathBuf, String> {
    let directory = crate::paths::data_subdir("gates")?;

    Ok(directory.join(format!(
        "{}-deploy-gate-{}.json",
        slugify(project_name),
        unix_timestamp()
    )))
}''',
    },

    "report.rs": {
        "default_report_path": '''pub fn default_report_path(
    project_name: &str,
) -> Result<PathBuf, String> {
    let directory = crate::paths::data_subdir("reports")?;

    Ok(directory.join(format!(
        "{}-deploy-report-{}.md",
        slugify(project_name),
        unix_timestamp()
    )))
}''',
    },
}


def fail(message: str) -> None:
    print(
        f"Error: {message}",
        file=sys.stderr,
    )

    raise SystemExit(1)


def closing_brace(
    source: str,
    opening: int,
) -> int:
    depth = 0
    index = opening
    state = "code"
    block_depth = 0

    while index < len(source):
        char = source[index]

        next_char = (
            source[index + 1]
            if index + 1 < len(source)
            else ""
        )

        if state == "code":
            if char == "/" and next_char == "/":
                state = "line_comment"
                index += 2
                continue

            if char == "/" and next_char == "*":
                state = "block_comment"
                block_depth = 1
                index += 2
                continue

            if char == '"':
                state = "string"
                index += 1
                continue

            if char == "'":
                state = "char"
                index += 1
                continue

            if char == "{":
                depth += 1

            elif char == "}":
                depth -= 1

                if depth == 0:
                    return index

            index += 1
            continue

        if state == "line_comment":
            if char == "\n":
                state = "code"

            index += 1
            continue

        if state == "block_comment":
            if char == "/" and next_char == "*":
                block_depth += 1
                index += 2
                continue

            if char == "*" and next_char == "/":
                block_depth -= 1
                index += 2

                if block_depth == 0:
                    state = "code"

                continue

            index += 1
            continue

        if state in {"string", "char"}:
            if char == "\\":
                index += 2
                continue

            if (
                state == "string"
                and char == '"'
            ):
                state = "code"

            elif (
                state == "char"
                and char == "'"
            ):
                state = "code"

            index += 1

    fail(
        "No se encontró la llave final "
        "de una función."
    )

    return -1


def replace_function(
    source: str,
    name: str,
    replacement: str,
) -> str:
    pattern = re.compile(
        rf"(?m)^pub\s+fn\s+{re.escape(name)}\b"
    )

    match = pattern.search(source)

    if match is None:
        fail(
            f"No se encontró la función {name}."
        )

    opening = source.find(
        "{",
        match.end(),
    )

    if opening == -1:
        fail(
            f"No se encontró el cuerpo de {name}."
        )

    ending = closing_brace(
        source,
        opening,
    )

    return (
        source[:match.start()]
        + replacement
        + source[ending + 1:]
    )


def ensure_paths_module(
    source: str,
) -> str:
    if re.search(
        r"(?m)^pub\s+mod\s+paths\s*;",
        source,
    ):
        return source

    modules = list(
        re.finditer(
            r"(?m)^pub\s+mod\s+"
            r"[A-Za-z0-9_]+\s*;\s*$",
            source,
        )
    )

    if not modules:
        fail(
            "No se encontró la lista de "
            "módulos de src/lib.rs."
        )

    position = modules[-1].end()

    return (
        source[:position]
        + "\npub mod paths;"
        + source[position:]
    )


def create_backup(
    files: list[Path],
) -> Path:
    timestamp = datetime.now().strftime(
        "%Y%m%d-%H%M%S"
    )

    backup_directory = (
        Path(tempfile.gettempdir())
        / f"opsdeck-paths-{timestamp}"
    )

    backup_directory.mkdir(
        parents=True,
        exist_ok=False,
    )

    for source_file in files:
        destination = (
            backup_directory
            / source_file.name
        )

        shutil.copy2(
            source_file,
            destination,
        )

    return backup_directory


def main() -> None:
    cargo_file = ROOT / "Cargo.toml"

    if not cargo_file.is_file():
        fail(
            "Guarda este script dentro de "
            "la carpeta scripts/ de OpsDeck."
        )

    paths_file = SRC / "paths.rs"

    if not paths_file.is_file():
        fail(
            "Falta src/paths.rs. Créalo antes "
            "de ejecutar esta migración."
        )

    files = [
        SRC / filename
        for filename in REPLACEMENTS
    ]

    missing_files = [
        str(path.relative_to(ROOT))
        for path in files
        if not path.is_file()
    ]

    if missing_files:
        fail(
            "Faltan archivos: "
            + ", ".join(missing_files)
        )

    backup_directory = create_backup(
        files
    )

    for filename, functions in REPLACEMENTS.items():
        path = SRC / filename

        source = path.read_text(
            encoding="utf-8"
        )

        for (
            function_name,
            replacement,
        ) in functions.items():
            source = replace_function(
                source,
                function_name,
                replacement,
            )

        source = re.sub(
            r"(?m)^[ \t]*use[ \t]+"
            r"std::env;[ \t]*\n",
            "",
            source,
        )

        if filename == "lib.rs":
            source = ensure_paths_module(
                source
            )

        path.write_text(
            source,
            encoding="utf-8",
        )

        print(
            f"Actualizado: "
            f"{path.relative_to(ROOT)}"
        )

    print()
    print("Migración completada.")
    print(
        f"Respaldo: {backup_directory}"
    )
    print()


if __name__ == "__main__":
    main()