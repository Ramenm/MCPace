#!/usr/bin/env python3

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import time
import zipfile
from collections import defaultdict
from collections.abc import Iterable
from dataclasses import dataclass, field
from datetime import UTC, datetime
from pathlib import Path

# --- “Авто-мусор”: папки/файлы, которые почти всегда генерятся и не нужны в “чистом” архиве.
JUNK_DIR_NAMES = {
    ".git",
    ".hg",
    ".svn",
    ".idea",
    ".vscode",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".tox",
    ".nox",
    ".pre-commit-cache",
    ".cache",
    ".gradle",
    ".venv",
    "venv",  # NB: "env" убрано намеренно (слишком общее имя)
    "node_modules",
    ".next",
    ".nuxt",
    ".svelte-kit",
    ".turbo",
    ".parcel-cache",
    "dist",
    "build",
    "out",
    "target",
}

JUNK_FILE_NAMES = {
    ".ds_store",
    "thumbs.db",
    "desktop.ini",
}

JUNK_EXTS = {
    # python junk
    ".pyc",
    ".pyo",
    ".pyd",
    # logs / temp / editor swap
    ".log",
    ".tmp",
    ".swp",
    ".swo",
    # archives (обычно не надо складывать архив в архив)
    ".zip",
    ".7z",
    ".rar",
    ".tar",
    ".gz",
    ".bz2",
    ".xz",
}

DEFAULT_MAX_FILE_BYTES = 15 * 1024 * 1024  # 15 MB

# zip требует год >= 1980
ZIP_DT = (1980, 1, 1, 0, 0, 0)

# Какие env-шаблоны “безопасно” включать даже если они в .gitignore
ENV_TEMPLATE_SUFFIXES = ("example", "sample", "template", "dist")

# Сколько записей о дропнутых файлах сохранять в отчёт (чтобы не раздувать json)
REPORT_SAMPLES_LIMIT = 200


# -------------------------------------------------------------------
# Статистика / отчёт
# -------------------------------------------------------------------


@dataclass
class PackStats:
    input: str
    output: str
    mode: str  # "dir" or "zip"
    max_bytes: int
    allow_dotenv: bool
    used_git: bool = False
    generated_at_utc: str = field(default_factory=lambda: datetime.now(UTC).isoformat())

    kept: int = 0
    dropped: int = 0
    kept_bytes: int = 0
    dropped_bytes: int = 0

    dropped_reasons: dict[str, int] = field(default_factory=lambda: defaultdict(int))
    dropped_samples: list[dict[str, object]] = field(default_factory=list)

    def add_drop(self, reason: str, rel: str | None = None, size: int | None = None) -> None:
        self.dropped += 1
        if size is not None and size >= 0:
            self.dropped_bytes += int(size)
        self.dropped_reasons[reason] += 1

        if rel and len(self.dropped_samples) < REPORT_SAMPLES_LIMIT:
            item = {"path": rel, "reason": reason}
            if size is not None and size >= 0:
                item["size"] = int(size)
            self.dropped_samples.append(item)

    def add_keep(self, size: int) -> None:
        self.kept += 1
        self.kept_bytes += int(size)

    def to_jsonable(self) -> dict[str, object]:
        return {
            "input": self.input,
            "output": self.output,
            "mode": self.mode,
            "max_bytes": self.max_bytes,
            "allow_dotenv": self.allow_dotenv,
            "used_git": self.used_git,
            "generated_at_utc": self.generated_at_utc,
            "kept": self.kept,
            "dropped": self.dropped,
            "kept_bytes": self.kept_bytes,
            "dropped_bytes": self.dropped_bytes,
            "dropped_reasons": dict(self.dropped_reasons),
            "dropped_samples": self.dropped_samples,
        }


# -------------------------------------------------------------------
# Вспомогательные функции
# -------------------------------------------------------------------


def now_stamp() -> str:
    return time.strftime("%Y%m%d_%H%M%S")


def make_output_name(base: str) -> str:
    return f"{base}_clean_{now_stamp()}.zip"


def is_unsafe_zip_relpath(rel: str) -> bool:
    """
    Защита от zip-slip / странных абсолютных путей.
    """
    rel = rel.replace("\\", "/")
    if not rel or rel.startswith("/") or rel.startswith("\\"):
        return True
    if re.match(r"^[A-Za-z]:", rel):  # C:...
        return True
    parts = [p for p in rel.split("/") if p]
    return any(p == ".." for p in parts)


def looks_like_dotenv_secret(name: str) -> bool:
    """
    True для ".env" и ".env.*" кроме шаблонов:
      .env.example, .env.local.example, .env.sample, .env.template, .env.dist, etc.
    """
    n = name.lower()
    if n == ".env" or n.startswith(".env."):
        return all(not n.endswith(f".{suf}") for suf in ENV_TEMPLATE_SUFFIXES)
    return False


def strip_leading_dot_slash(path: str) -> str:
    """
    Удаляет ТОЛЬКО префикс "./" (возможно повторяющийся), но НЕ трогает файлы,
    начинающиеся с точки (".gitignore", ".github/...").
    """
    p = path.replace("\\", "/")
    while p.startswith("./"):
        p = p[2:]
    return p


# -------------------------------------------------------------------
# Фильтрация “оставить/удалить”
# -------------------------------------------------------------------


def drop_reason_for_file(
    rel_posix: str, size: int, max_bytes: int, allow_dotenv: bool, dotenv_fallback_filter: bool
) -> str | None:
    """
    Возвращает причину дропа или None если файл надо оставить.
    """
    if not rel_posix or rel_posix.endswith("/"):
        return "not_a_file"

    rel_posix = rel_posix.replace("\\", "/")
    rel_lower = rel_posix.lower()
    name = rel_lower.rsplit("/", 1)[-1]

    if name in JUNK_FILE_NAMES:
        return "junk_file_name"

    # safety net: только когда Git недоступен (dotenv_fallback_filter=True)
    if dotenv_fallback_filter and (not allow_dotenv) and looks_like_dotenv_secret(name):
        return "dotenv_filtered"

    ext = Path(name).suffix.lower()
    if ext in JUNK_EXTS:
        return "junk_ext"

    if size < 0:
        return "bad_size"

    if size > max_bytes:
        return "too_big"

    return None


# -------------------------------------------------------------------
# Git-интеграция
# -------------------------------------------------------------------


def try_git_toplevel(start_dir: Path) -> Path | None:
    """
    Если внутри git — возвращаем корень.
    """
    try:
        res = subprocess.run(
            ["git", "-C", str(start_dir), "rev-parse", "--show-toplevel"],
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            check=True,
        )
        p = res.stdout.strip()
        return Path(p).resolve() if p else None
    except (FileNotFoundError, OSError, subprocess.CalledProcessError):
        return None


def _git_ls_files(repo_root: Path, args: list[str]) -> list[str] | None:
    """
    Универсальный helper для `git ls-files ... -z`.
    Возвращает список путей (строки) относительно repo_root.
    """
    try:
        res = subprocess.run(
            ["git", "-C", str(repo_root), "ls-files", *args],
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            check=True,
        )
        raw = res.stdout
        if not raw:
            return []
        out: list[str] = []
        for b in raw.split(b"\x00"):
            if b:
                out.append(b.decode("utf-8", errors="replace"))
        return out
    except (FileNotFoundError, OSError, subprocess.CalledProcessError):
        return None


def build_force_include_specs(subpath: str) -> list[str]:
    """
    Строит pathspec-и для добора env-шаблонов, даже если они игнорируются.
    Используем :(glob) + варианты:
      <subpath>/.env*.example
      <subpath>/**/.env*.example
    """
    sub = subpath.strip("/")

    specs: list[str] = []
    for suf in ENV_TEMPLATE_SUFFIXES:
        pat = f".env*.{suf}"
        if sub in ("", "."):
            specs.append(f":(glob){pat}")
            specs.append(f":(glob)**/{pat}")
        else:
            specs.append(f":(glob){sub}/{pat}")
            specs.append(f":(glob){sub}/**/{pat}")
    return specs


def try_git_file_list(input_dir: Path) -> tuple[Path | None, list[str] | None, list[str] | None]:
    """
    Возвращает (repo_root, files_main, files_ignored_templates)
    - files_main: tracked + untracked (без игнорируемых), ограниченные input_dir
    - files_ignored_templates: игнорируемые (untracked) env-шаблоны, тоже ограниченные input_dir
    Если git недоступен: (None, None, None)
    """
    repo_root = try_git_toplevel(input_dir)
    if not repo_root:
        return None, None, None

    try:
        rel_to_repo = input_dir.resolve().relative_to(repo_root)
        subpath = rel_to_repo.as_posix() if str(rel_to_repo) != "." else "."
    except ValueError:
        subpath = "."

    # tracked (-c) + others/untracked (-o), но без игноров (--exclude-standard)
    files_main = _git_ls_files(repo_root, ["-co", "--exclude-standard", "-z", "--", subpath])
    if files_main is None:
        return repo_root, None, None

    # добираем игнорируемые env-шаблоны (untracked ignored): -o -i
    force_specs = build_force_include_specs(subpath)
    files_ignored_templates: list[str] | None = []
    if force_specs:
        files_ignored_templates = _git_ls_files(
            repo_root,
            ["-oi", "--exclude-standard", "-z", "--", *force_specs],
        )
        if files_ignored_templates is None:
            files_ignored_templates = []

    return repo_root, files_main, files_ignored_templates


# -------------------------------------------------------------------
# Итерация файлов директории
# -------------------------------------------------------------------


def iter_files_from_dir(
    root: Path, output_zip: Path, stats: PackStats
) -> Iterable[tuple[Path, str, bool]]:
    """
    Отдаёт (abs_path, rel_posix_in_zip, dotenv_fallback_filter)
    - Если git доступен — берём список через git (deterministic), плюс добор env-шаблонов.
    - Если git недоступен — os.walk (sorted) + включаем safety net для .env (по умолчанию).
    """
    out_zip_resolved = output_zip.resolve()
    root = root.resolve()

    repo_root, files_main, files_ignored_templates = try_git_file_list(root)

    if repo_root and files_main is not None:
        stats.used_git = True
        # input_dir относительно repo_root
        try:
            sub_rel = root.relative_to(repo_root).as_posix()
        except ValueError:
            sub_rel = "."

        all_files = set(files_main)
        if files_ignored_templates:
            all_files.update(files_ignored_templates)

        for rel in sorted(all_files):
            rel_posix = strip_leading_dot_slash(rel.replace("\\", "/"))
            if not rel_posix or is_unsafe_zip_relpath(rel_posix):
                stats.add_drop("unsafe_path_in_git", rel=rel_posix)
                continue

            abs_path = (repo_root / rel_posix).resolve()
            if abs_path == out_zip_resolved:
                continue
            if not abs_path.is_file():
                continue

            # arcname в zip должен быть относительно input_dir (root), а не repo_root
            if sub_rel in ("", "."):
                arcname = rel_posix
            else:
                prefix = sub_rel.rstrip("/") + "/"
                if not rel_posix.startswith(prefix):
                    # на всякий: если вдруг git вернул что-то вне input_dir
                    continue
                arcname = rel_posix[len(prefix) :]

            arcname = strip_leading_dot_slash(arcname.replace("\\", "/"))
            if not arcname or is_unsafe_zip_relpath(arcname):
                stats.add_drop("unsafe_arcname", rel=arcname)
                continue

            yield (
                abs_path,
                arcname,
                False,
            )  # dotenv_fallback_filter=False (в git-режиме не фильтруем)

        return

    # fallback: os.walk (детерминированный порядок)
    root_str = str(root)
    for dirpath, dirnames, filenames in os.walk(root_str, topdown=True, followlinks=False):
        dirnames[:] = [d for d in dirnames if d not in JUNK_DIR_NAMES]
        dirnames.sort()
        filenames.sort()

        rel_dir = os.path.relpath(dirpath, root_str)
        rel_dir = "" if rel_dir == "." else rel_dir

        for fname in filenames:
            rel_path = os.path.normpath(os.path.join(rel_dir, fname))
            rel_posix = rel_path.replace(os.sep, "/")
            if not rel_posix or is_unsafe_zip_relpath(rel_posix):
                stats.add_drop("unsafe_path_walk", rel=rel_posix)
                continue

            abs_path = (root / rel_path).resolve()
            if abs_path == out_zip_resolved:
                continue
            if abs_path.is_file():
                yield abs_path, rel_posix, True  # dotenv_fallback_filter=True


# -------------------------------------------------------------------
# Добавление в ZIP
# -------------------------------------------------------------------


def zip_add_stream(
    zout: zipfile.ZipFile,
    arcname: str,
    src_file,
    st_mode: int | None = None,
) -> None:
    zi = zipfile.ZipInfo(arcname)
    zi.date_time = ZIP_DT
    zi.compress_type = zout.compression
    if st_mode is not None:
        zi.external_attr = (st_mode & 0xFFFF) << 16
    with zout.open(zi, "w") as dst:
        shutil.copyfileobj(src_file, dst, length=1024 * 1024)


def open_zip_write(output_zip: Path) -> zipfile.ZipFile:
    """
    compresslevel поддерживается не во всех версиях Python (добавили в 3.7),
    поэтому пробуем и откатываемся.
    """
    kwargs = dict(mode="w", compression=zipfile.ZIP_DEFLATED)
    try:
        return zipfile.ZipFile(output_zip, compresslevel=1, **kwargs)
    except TypeError:
        return zipfile.ZipFile(output_zip, **kwargs)


# -------------------------------------------------------------------
# Упаковка директории
# -------------------------------------------------------------------


def pack_dir_fast(
    input_dir: Path, output_zip: Path, max_bytes: int, allow_dotenv: bool, report_path: Path | None
) -> None:
    root = input_dir.resolve()
    output_zip.parent.mkdir(parents=True, exist_ok=True)

    stats = PackStats(
        input=str(root),
        output=str(output_zip.resolve()),
        mode="dir",
        max_bytes=max_bytes,
        allow_dotenv=allow_dotenv,
    )

    with open_zip_write(output_zip) as zout:
        for abs_path, rel_posix, dotenv_fallback_filter in iter_files_from_dir(
            root, output_zip, stats
        ):
            # режем мусорные директории по частям пути
            if any(part in JUNK_DIR_NAMES for part in rel_posix.split("/")):
                stats.add_drop("junk_dir", rel=rel_posix)
                continue

            try:
                st = abs_path.stat()
                size = int(st.st_size)
            except OSError:
                stats.add_drop("stat_failed", rel=rel_posix)
                continue

            reason = drop_reason_for_file(
                rel_posix, size, max_bytes, allow_dotenv, dotenv_fallback_filter
            )
            if reason:
                stats.add_drop(reason, rel=rel_posix, size=size)
                continue

            try:
                with abs_path.open("rb") as f:
                    zip_add_stream(
                        zout,
                        rel_posix,
                        f,
                        st_mode=getattr(st, "st_mode", None),
                    )
                stats.add_keep(size)
            except (OSError, RuntimeError, ValueError, zipfile.BadZipFile):
                stats.add_drop("read_or_zip_failed", rel=rel_posix, size=size)

    print(f"OK: {output_zip}")
    print(f"kept={stats.kept} dropped={stats.dropped}")
    print(f"kept_bytes={stats.kept_bytes} dropped_bytes={stats.dropped_bytes}")
    print(f"used_git={stats.used_git}")

    if report_path:
        report_path.parent.mkdir(parents=True, exist_ok=True)
        report_path.write_text(
            json.dumps(stats.to_jsonable(), ensure_ascii=False, indent=2), encoding="utf-8"
        )
        print(f"report: {report_path}")


# -------------------------------------------------------------------
# Очистка существующего ZIP
# -------------------------------------------------------------------


def detect_single_toplevel_prefix(names: Iterable[str]) -> str:
    first_parts = set()
    cleaned = []
    for n in names:
        if not n or n.endswith("/"):
            continue
        if n.startswith("__MACOSX/"):
            continue
        cleaned.append(n)
        parts = n.split("/")
        if parts and parts[0]:
            first_parts.add(parts[0])

    if len(first_parts) != 1:
        return ""
    prefix = next(iter(first_parts)) + "/"
    for n in cleaned:
        if not n.startswith(prefix):
            return ""
    return prefix


def clean_zip_fast(
    input_zip: Path, output_zip: Path, max_bytes: int, allow_dotenv: bool, report_path: Path | None
) -> None:
    output_zip.parent.mkdir(parents=True, exist_ok=True)

    stats = PackStats(
        input=str(input_zip.resolve()),
        output=str(output_zip.resolve()),
        mode="zip",
        max_bytes=max_bytes,
        allow_dotenv=allow_dotenv,
    )

    with zipfile.ZipFile(input_zip, "r") as zin, open_zip_write(output_zip) as zout:
        infos = [i for i in zin.infolist() if not i.is_dir()]
        prefix = detect_single_toplevel_prefix(i.filename.replace("\\", "/") for i in infos)

        # Детерминированный порядок: сортируем по будущему имени в архиве
        sortable: list[tuple[str, zipfile.ZipInfo]] = []
        for info in infos:
            name = info.filename.replace("\\", "/")
            if not name or name.endswith("/"):
                continue
            if name.startswith("__MACOSX/"):
                continue

            rel = name
            if prefix and rel.startswith(prefix):
                rel = rel[len(prefix) :]

            rel = strip_leading_dot_slash(rel.replace("\\", "/"))
            if not rel:
                continue

            sortable.append((rel, info))

        sortable.sort(key=lambda x: x[0])

        for rel0, info in sortable:
            rel = rel0

            if not rel or is_unsafe_zip_relpath(rel):
                stats.add_drop("unsafe_path_in_zip", rel=rel)
                continue

            if any(part in JUNK_DIR_NAMES for part in rel.split("/")):
                stats.add_drop("junk_dir", rel=rel, size=int(info.file_size or 0))
                continue

            size = int(info.file_size or 0)

            # Для режима очистки ZIP мы НЕ знаем, был ли оригинальный проект под git,
            # поэтому safety-net по .env применяем ВСЕГДА если allow_dotenv=False.
            # (Иначе можно случайно упаковать реальные секреты из чужого zip)
            dotenv_fallback_filter = True

            reason = drop_reason_for_file(
                rel, size, max_bytes, allow_dotenv, dotenv_fallback_filter
            )
            if reason:
                stats.add_drop(reason, rel=rel, size=size)
                continue

            try:
                with zin.open(info, "r") as f:
                    zi = zipfile.ZipInfo(rel)
                    zi.date_time = ZIP_DT
                    zi.compress_type = zout.compression
                    zi.external_attr = info.external_attr

                    with zout.open(zi, "w") as dst:
                        shutil.copyfileobj(f, dst, length=1024 * 1024)

                stats.add_keep(size)
            except (OSError, RuntimeError, ValueError, zipfile.BadZipFile):
                stats.add_drop("read_or_zip_failed", rel=rel, size=size)

    print(f"OK: {output_zip}")
    print(f"kept={stats.kept} dropped={stats.dropped}")
    print(f"kept_bytes={stats.kept_bytes} dropped_bytes={stats.dropped_bytes}")

    if report_path:
        report_path.parent.mkdir(parents=True, exist_ok=True)
        report_path.write_text(
            json.dumps(stats.to_jsonable(), ensure_ascii=False, indent=2), encoding="utf-8"
        )
        print(f"report: {report_path}")


# -------------------------------------------------------------------
# CLI
# -------------------------------------------------------------------


def _try_parse_int(s: str) -> int | None:
    try:
        return int(s)
    except ValueError:
        return None


def print_help() -> None:
    exe = Path(sys.argv[0]).name
    print(
        f"""Usage:
  {exe} [input] [output] [max_bytes]
  {exe} [--report PATH] [--allow-dotenv] [--max-bytes N] [input] [output]
  {exe} -h | --help

Notes:
  - If input is omitted: picks newest *.zip in CWD (non *_clean_*.zip), else uses git root (or CWD).
  - max_bytes default: {DEFAULT_MAX_FILE_BYTES} (~15MB)
  - --report writes JSON report.
  - --allow-dotenv: include .env / .env.* in non-git fallback and when cleaning an existing zip.
    (env templates like .env.example are always allowed.)

Examples:
  {exe}
  {exe} ./my_project
  {exe} ./my_project out.zip
  {exe} ./my_project out.zip 15728640
  {exe} --report pack_report.json ./my_project
  {exe} --allow-dotenv --report r.json ./some_folder
"""
    )


def parse_cli(argv: list[str]) -> tuple[str | None, str | None, int, str | None, bool, str | None]:
    """
    Returns: (input_str, out_str, max_bytes, report_str, allow_dotenv, err)
    Keeps backward compatibility with positional [input] [output] [max_bytes].
    """
    if any(a in ("-h", "--help") for a in argv):
        print_help()
        return None, None, DEFAULT_MAX_FILE_BYTES, None, False, "EXIT_HELP"

    allow_dotenv = False
    report_str: str | None = None
    max_bytes: int | None = None

    args: list[str] = []
    i = 0
    while i < len(argv):
        a = argv[i]
        if a == "--allow-dotenv":
            allow_dotenv = True
            i += 1
            continue

        if a in ("--report", "-r"):
            if i + 1 >= len(argv):
                return (
                    None,
                    None,
                    DEFAULT_MAX_FILE_BYTES,
                    None,
                    allow_dotenv,
                    "ERROR: --report requires PATH",
                )
            report_str = argv[i + 1]
            i += 2
            continue

        if a.startswith("--report="):
            report_str = a.split("=", 1)[1].strip() or None
            i += 1
            continue

        if a in ("--max-bytes", "--max"):
            if i + 1 >= len(argv):
                return (
                    None,
                    None,
                    DEFAULT_MAX_FILE_BYTES,
                    report_str,
                    allow_dotenv,
                    "ERROR: --max-bytes requires N",
                )
            v = _try_parse_int(argv[i + 1])
            if v is None:
                return (
                    None,
                    None,
                    DEFAULT_MAX_FILE_BYTES,
                    report_str,
                    allow_dotenv,
                    "ERROR: --max-bytes must be integer",
                )
            max_bytes = v
            i += 2
            continue

        if a.startswith("--max-bytes=") or a.startswith("--max="):
            v = _try_parse_int(a.split("=", 1)[1])
            if v is None:
                return (
                    None,
                    None,
                    DEFAULT_MAX_FILE_BYTES,
                    report_str,
                    allow_dotenv,
                    "ERROR: --max-bytes must be integer",
                )
            max_bytes = v
            i += 1
            continue

        args.append(a)
        i += 1

    # backward-compatible: trailing int as max_bytes if --max-bytes не задан
    if max_bytes is None and args:
        maybe = _try_parse_int(args[-1])
        if maybe is not None:
            max_bytes = maybe
            args = args[:-1]

    if max_bytes is None:
        max_bytes = DEFAULT_MAX_FILE_BYTES

    if max_bytes <= 0:
        return (
            None,
            None,
            DEFAULT_MAX_FILE_BYTES,
            report_str,
            allow_dotenv,
            "ERROR: max_bytes must be > 0",
        )

    if len(args) == 0:
        return None, None, max_bytes, report_str, allow_dotenv, None
    if len(args) == 1:
        return args[0], None, max_bytes, report_str, allow_dotenv, None
    if len(args) == 2:
        return args[0], args[1], max_bytes, report_str, allow_dotenv, None

    return None, None, DEFAULT_MAX_FILE_BYTES, report_str, allow_dotenv, "ERROR: too many arguments"


def pick_input_auto(cwd: Path) -> tuple[str, Path]:
    zips: list[Path] = []
    for p in cwd.glob("*.zip"):
        if re.search(r"_clean_\d{8}_\d{6}\.zip$", p.name):
            continue
        zips.append(p)

    if zips:
        zips.sort(key=lambda x: x.stat().st_mtime, reverse=True)
        return "zip", zips[0]

    root = try_git_toplevel(cwd) or cwd
    return "dir", root


def main() -> int:
    input_str, out_str, max_bytes, report_str, allow_dotenv, err = parse_cli(sys.argv[1:])
    if err == "EXIT_HELP":
        return 0
    if err:
        print(err, file=sys.stderr)
        return 2

    cwd = Path.cwd()

    if input_str:
        src = Path(input_str).expanduser().resolve()
        if not src.exists():
            print(f"ERROR: input not found: {src}", file=sys.stderr)
            return 2
        mode = "zip" if (src.is_file() and src.suffix.lower() == ".zip") else "dir"
    else:
        mode, src = pick_input_auto(cwd)

    out_zip = Path(out_str).expanduser().resolve() if out_str else None
    report_path = Path(report_str).expanduser().resolve() if report_str else None

    if mode == "dir":
        root = src
        base = root.name or "project"
        final_out = out_zip if out_zip else (root / make_output_name(base))
        pack_dir_fast(
            root, final_out, max_bytes=max_bytes, allow_dotenv=allow_dotenv, report_path=report_path
        )
        return 0

    final_out = out_zip if out_zip else (src.parent / make_output_name(src.stem))
    clean_zip_fast(
        src, final_out, max_bytes=max_bytes, allow_dotenv=allow_dotenv, report_path=report_path
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
