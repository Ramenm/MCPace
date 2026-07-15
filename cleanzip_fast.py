#!/usr/bin/env python3
# -*- coding: utf-8 -*-

from __future__ import annotations

import fnmatch
import json
import os
import re
import stat
import sys
import time
import zipfile
import shutil
import subprocess
import tempfile
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Iterable, Optional, Tuple, List, Dict
from collections import defaultdict

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
    ".cache",
    ".gradle",
    # Generated local/agent artifacts. Keep reviewable reports/ in source bundles;
    # path-prefix rules below remove machine-local runtime state instead.
    "backups",
    "logs",
    ".claude",
    ".omc",
    ".omx",
    ".ops-backups",
    ".runtime",
    ".sync-backups",
    ".tools",
    "memory-bank",
    "merge-preserved",
    "generated",
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

JUNK_FILE_PATTERNS = {
    "FINAL_RECHECK_REPORT_*.md",
    "FULL_RECOVERY_REPORT_*.md",
    "MERGE_MANIFEST*.json",
    "MERGE_REPORT*.md",
    "RECOVERY_REBUILD_MANIFEST*.json",
    "RECOVERY_VALIDATION_*.md",
    "REPAIR_REPORT_*.md",
    "SECOND_PASS_VALIDATION_*.json",
    "SECOND_PASS_VALIDATION_*.md",
    "THIRD_PASS_*.md",
    "THIRD_PASS_MANIFEST*.json",
}

RUNTIME_FILE_PATTERNS = {
    "*.sqlite",
    "*.sqlite3",
    "*.db",
    "mcpace-autostart.vbs",
    "mcpace-serve-*.exe",
    "project-registry.json",
    "*.partial.jsonl",
}

# Static source fixtures that intentionally look database-like. Keep this list
# aligned with scripts/lib/source-archive-policy.mjs so the generic cleaner does
# not delete reviewable project inputs while still stripping runtime state.
ALLOWED_RUNTIME_FILE_PATHS = {
    "metadata/metadata.sqlite3",
}


def is_allowed_runtime_file_path(rel_posix: str) -> bool:
    rel = rel_posix.replace("\\", "/").lstrip("./").lower()
    return any(
        rel == allowed or rel.endswith("/" + allowed)
        for allowed in ALLOWED_RUNTIME_FILE_PATHS
    )


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

# Full path prefixes that are generated runtime/build state even when the
# individual path components are ordinary project names. These are checked
# before extension filtering so source fixtures under eval/ or docs/ are not
# affected.
JUNK_PATH_PREFIXES = (
    "data/runtime/",
    "data/server-state/",
    "logs/",
    "backups/",
    "dist/",
    "target/",
)

JUNK_PATH_EXACT = {
    "data/runtime",
    "data/server-state",
    "logs",
    "backups",
    "dist",
    "target",
}


def generated_state_path_reason(rel_posix: str) -> Optional[str]:
    rel = rel_posix.replace("\\", "/").lstrip("./")
    if is_allowed_runtime_file_path(rel):
        return None
    if rel in JUNK_PATH_EXACT or any(
        rel.startswith(prefix) for prefix in JUNK_PATH_PREFIXES
    ):
        return "generated_runtime_or_build_state"
    lower = rel.lower()
    if re.fullmatch(r"catalog/registry-cache(?:-search-[a-f0-9]+)?\.json", lower):
        return "generated_registry_cache"
    if lower.endswith(".sqlite") or lower.endswith(".sqlite3") or lower.endswith(".db"):
        return "generated_database_state"
    if lower.startswith("data/runtime/") and (
        lower.endswith(".exe") or lower.endswith(".vbs")
    ):
        return "generated_runtime_executable_or_launcher"
    return None


EXECUTABLE_BASENAMES = {
    "start.sh",
    "stop.sh",
    "pipctl.sh",
    "apply_migrations.sh",
    "collect_pip_logs.sh",
    "pip_diag.sh",
}

EXECUTABLE_RELATIVE_PATHS = {
    "packages/npm/cli/bin/mcpace.js",
}


def normalized_zip_mode(arcname: str, st_mode: Optional[int] = None) -> int:
    """Return deterministic unix mode bits for zip entries.

    Some input ZIPs arrive with FAT/Windows metadata and lose executable bits.
    Shell entrypoints must be executable after unzip because docs use ./start.sh,
    ./pipctl.sh, and shell scripts directly.
    """
    rel = arcname.replace("\\", "/")
    name = rel.rsplit("/", 1)[-1]
    if rel in EXECUTABLE_RELATIVE_PATHS:
        return 0o100755
    if name in EXECUTABLE_BASENAMES or name.endswith(".sh"):
        return 0o100755
    # When cleaning archives created on FAT/Windows, executable bits are often
    # missing. Release checks intentionally treat scripts with shebangs as
    # executable entrypoints; infer the common release-script locations here so
    # a clean ZIP can be extracted and used directly on Linux/macOS.
    if rel == "cleanzip_fast.py" or (
        rel.startswith("scripts/") and name.endswith(".py")
    ):
        return 0o100755
    if st_mode is not None and st_mode & 0o111:
        return 0o100755
    return 0o100644


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
    generated_at_utc: str = field(
        default_factory=lambda: datetime.now(timezone.utc).isoformat()
    )

    kept: int = 0
    dropped: int = 0
    kept_bytes: int = 0
    dropped_bytes: int = 0

    dropped_reasons: Dict[str, int] = field(default_factory=lambda: defaultdict(int))
    dropped_samples: List[Dict[str, object]] = field(default_factory=list)

    def add_drop(
        self,
        reason: str,
        rel: Optional[str] = None,
        size: Optional[int] = None,
        detail: Optional[str] = None,
    ) -> None:
        self.dropped += 1
        if size is not None and size >= 0:
            self.dropped_bytes += size
        self.dropped_reasons[reason] += 1

        if rel and len(self.dropped_samples) < REPORT_SAMPLES_LIMIT:
            item: Dict[str, object] = {"path": rel, "reason": reason}
            if size is not None and size >= 0:
                item["size"] = size
            if detail:
                item["detail"] = detail[:240]
            self.dropped_samples.append(item)

    def add_keep(self, size: int) -> None:
        self.kept += 1
        self.kept_bytes += size

    def to_jsonable(self) -> Dict[str, object]:
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
        for suf in ENV_TEMPLATE_SUFFIXES:
            if n.endswith(f".{suf}"):
                return False
        return True
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


def path_matches_prefix(rel_posix: str, prefix: str) -> bool:
    rel = strip_leading_dot_slash(rel_posix.replace("\\", "/")).strip("/").lower()
    normalized_prefix = prefix.replace("\\", "/").strip("/").lower()
    return rel == normalized_prefix or rel.startswith(f"{normalized_prefix}/")


def path_has_junk_prefix(rel_posix: str) -> bool:
    return any(path_matches_prefix(rel_posix, prefix) for prefix in JUNK_PATH_PREFIXES)


# -------------------------------------------------------------------
# Фильтрация “оставить/удалить”
# -------------------------------------------------------------------


def matches_junk_path_prefix(rel_posix: str) -> Optional[str]:
    rel = strip_leading_dot_slash(rel_posix.replace("\\", "/")).strip("/").lower()
    for prefix in JUNK_PATH_PREFIXES:
        p = prefix.lower().strip("/")
        if rel == p or rel.startswith(p + "/"):
            return prefix
    return None


def matches_runtime_file_pattern(rel_posix: str) -> Optional[str]:
    rel = strip_leading_dot_slash(rel_posix.replace("\\", "/"))
    name = rel.lower().rsplit("/", 1)[-1]
    for pattern in RUNTIME_FILE_PATTERNS:
        if fnmatch.fnmatch(name, pattern.lower()):
            return pattern
    if "/tool-list-cache/" in f"/{rel.lower()}/":
        return "tool-list-cache"
    return None


def drop_reason_for_file(
    rel_posix: str,
    size: int,
    max_bytes: int,
    allow_dotenv: bool,
    dotenv_fallback_filter: bool,
) -> Optional[str]:
    """
    Возвращает причину дропа или None если файл надо оставить.
    """
    if not rel_posix or rel_posix.endswith("/"):
        return "not_a_file"

    rel_posix = rel_posix.replace("\\", "/")
    rel_lower = rel_posix.lower()
    name = rel_lower.rsplit("/", 1)[-1]

    if is_allowed_runtime_file_path(rel_lower):
        return None

    if any(path_matches_prefix(rel_lower, prefix) for prefix in JUNK_PATH_PREFIXES):
        return "junk_path_prefix"

    if rel_posix == "ops/self-owned/proxies.csv":
        return "self_owned_inventory_secret"

    if path_has_junk_prefix(rel_posix):
        return "junk_path_prefix"

    if matches_runtime_file_pattern(rel_posix):
        return "runtime_file_pattern"

    if name in JUNK_FILE_NAMES:
        return "junk_file_name"

    # Recovery/merge reports are useful during reconstruction but should not be
    # embedded into clean release archives. Match only archive-relative names.
    if any(fnmatch.fnmatch(rel_posix, pattern) for pattern in JUNK_FILE_PATTERNS):
        return "junk_file_pattern"

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


def try_git_toplevel(start_dir: Path) -> Optional[Path]:
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
    except Exception:
        return None


def _git_ls_files(repo_root: Path, args: List[str]) -> Optional[List[str]]:
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
        out: List[str] = []
        for b in raw.split(b"\x00"):
            if b:
                out.append(b.decode("utf-8", errors="replace"))
        return out
    except Exception:
        return None


def build_force_include_specs(subpath: str) -> List[str]:
    """
    Строит pathspec-и для добора env-шаблонов, даже если они игнорируются.
    Используем :(glob) + варианты:
      <subpath>/.env*.example
      <subpath>/**/.env*.example
    """
    sub = subpath.strip("/")

    specs: List[str] = []
    for suf in ENV_TEMPLATE_SUFFIXES:
        pat = f".env*.{suf}"
        if sub in ("", "."):
            specs.append(f":(glob){pat}")
            specs.append(f":(glob)**/{pat}")
        else:
            specs.append(f":(glob){sub}/{pat}")
            specs.append(f":(glob){sub}/**/{pat}")
    return specs


def try_git_file_list(
    input_dir: Path,
) -> Tuple[Optional[Path], Optional[List[str]], Optional[List[str]]]:
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
    except Exception:
        subpath = "."

    # tracked (-c) + others/untracked (-o), но без игноров (--exclude-standard)
    files_main = _git_ls_files(
        repo_root, ["-co", "--exclude-standard", "-z", "--", subpath]
    )
    if files_main is None:
        return repo_root, None, None

    # добираем игнорируемые env-шаблоны (untracked ignored): -o -i
    force_specs = build_force_include_specs(subpath)
    files_ignored_templates: Optional[List[str]] = []
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


def path_is_within(path: Path, root: Path) -> bool:
    try:
        path.relative_to(root)
        return True
    except ValueError:
        return False


def stat_open_fingerprint(value) -> Tuple[int, int, int, int, int]:
    return (
        value.st_dev,
        value.st_ino,
        value.st_mode,
        value.st_size,
        value.st_mtime_ns,
    )


def stat_stability_fingerprint(value) -> Tuple[int, int, int, int, int, int]:
    return (*stat_open_fingerprint(value), value.st_ctime_ns)


def path_entry_is_link_or_reparse(path: Path, entry_stat) -> bool:
    if stat.S_ISLNK(entry_stat.st_mode):
        return True
    if getattr(path, "is_junction", lambda: False)():
        return True
    reparse_flag = getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
    return bool(getattr(entry_stat, "st_file_attributes", 0) & reparse_flag)


def path_has_link_component(path: Path, root: Path) -> bool:
    try:
        relative = path.relative_to(root)
    except ValueError:
        return True
    current = root
    for part in relative.parts:
        current = current / part
        try:
            entry_stat = current.lstat()
        except OSError:
            return True
        if path_entry_is_link_or_reparse(current, entry_stat):
            return True
    return False


def open_regular_file_no_follow(path: Path, expected_stat):
    flags = os.O_RDONLY | getattr(os, "O_BINARY", 0) | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(path, flags)
    try:
        opened_stat = os.fstat(descriptor)
        if not stat.S_ISREG(opened_stat.st_mode):
            raise OSError("archive input is not a regular file")
        if stat_open_fingerprint(expected_stat) != stat_open_fingerprint(opened_stat):
            raise OSError("archive input changed while it was being opened")
        return os.fdopen(descriptor, "rb")
    except Exception:
        os.close(descriptor)
        raise


def read_regular_file_stable(path: Path, root: Path, expected_stat):
    if path_has_link_component(path, root):
        raise OSError("archive input contains a symlink or reparse-point component")
    staged = tempfile.SpooledTemporaryFile(max_size=8 * 1024 * 1024, mode="w+b")
    try:
        with open_regular_file_no_follow(path, expected_stat) as source:
            opened_before = os.fstat(source.fileno())
            shutil.copyfileobj(source, staged, length=1024 * 1024)
            opened_after = os.fstat(source.fileno())
        path_after = path.lstat()
        if path_has_link_component(path, root):
            raise OSError("archive input path changed to a symlink or reparse point")
        opened_fingerprint = stat_stability_fingerprint(opened_before)
        if (
            stat_stability_fingerprint(opened_after) != opened_fingerprint
            or stat_open_fingerprint(path_after) != stat_open_fingerprint(opened_after)
            or staged.tell() != opened_before.st_size
        ):
            raise OSError("archive input changed while it was being read")
        staged.seek(0)
        return staged
    except Exception:
        staged.close()
        raise


def iter_files_from_dir(
    root: Path, output_zip: Path, stats: PackStats
) -> Iterable[Tuple[Path, str, bool]]:
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
        except Exception:
            sub_rel = "."

        all_files = set(files_main)
        if files_ignored_templates:
            all_files.update(files_ignored_templates)

        for rel in sorted(all_files):
            rel_posix = strip_leading_dot_slash(rel.replace("\\", "/"))
            if not rel_posix or is_unsafe_zip_relpath(rel_posix):
                stats.add_drop("unsafe_path_in_git", rel=rel_posix)
                continue
            if path_has_junk_prefix(rel_posix):
                stats.add_drop("junk_path_prefix", rel=rel_posix)
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

            abs_path = repo_root / rel_posix
            try:
                entry_stat = abs_path.lstat()
                resolved_path = abs_path.resolve(strict=True)
            except OSError:
                stats.add_drop("stat_failed", rel=arcname)
                continue
            if path_has_link_component(abs_path, root):
                stats.add_drop("symlink_component", rel=arcname)
                continue
            if not stat.S_ISREG(entry_stat.st_mode):
                stats.add_drop("non_regular_file", rel=arcname)
                continue
            if not path_is_within(resolved_path, root):
                stats.add_drop("outside_input_root", rel=arcname)
                continue
            if resolved_path == out_zip_resolved:
                continue

            yield (
                abs_path,
                arcname,
                False,
            )  # dotenv_fallback_filter=False (в git-режиме не фильтруем)

        return

    # fallback: os.walk (детерминированный порядок)
    root_str = str(root)
    for dirpath, dirnames, filenames in os.walk(
        root_str, topdown=True, followlinks=False
    ):
        rel_dir = os.path.relpath(dirpath, root_str)
        rel_dir = "" if rel_dir == "." else rel_dir
        rel_dir_posix = rel_dir.replace(os.sep, "/")
        kept_dirs = []
        for d in dirnames:
            child_rel = d if not rel_dir_posix else f"{rel_dir_posix}/{d}"
            child_path = root / child_rel
            try:
                child_stat = child_path.lstat()
            except OSError:
                stats.add_drop("stat_failed", rel=child_rel)
                continue
            if path_entry_is_link_or_reparse(child_path, child_stat):
                stats.add_drop("symlink_dir", rel=child_rel)
                continue
            if d in JUNK_DIR_NAMES or path_has_junk_prefix(child_rel):
                stats.add_drop("junk_dir", rel=child_rel)
                continue
            kept_dirs.append(d)
        dirnames[:] = kept_dirs
        dirnames.sort()
        filenames.sort()

        for fname in filenames:
            rel_path = os.path.normpath(os.path.join(rel_dir, fname))
            rel_posix = rel_path.replace(os.sep, "/")
            if not rel_posix or is_unsafe_zip_relpath(rel_posix):
                stats.add_drop("unsafe_path_walk", rel=rel_posix)
                continue

            abs_path = root / rel_path
            try:
                entry_stat = abs_path.lstat()
                resolved_path = abs_path.resolve(strict=True)
            except OSError:
                stats.add_drop("stat_failed", rel=rel_posix)
                continue
            if path_has_link_component(abs_path, root):
                stats.add_drop("symlink_component", rel=rel_posix)
                continue
            if not stat.S_ISREG(entry_stat.st_mode):
                stats.add_drop("non_regular_file", rel=rel_posix)
                continue
            if not path_is_within(resolved_path, root):
                stats.add_drop("outside_input_root", rel=rel_posix)
                continue
            if resolved_path == out_zip_resolved:
                continue
            yield abs_path, rel_posix, True  # dotenv_fallback_filter=True


# -------------------------------------------------------------------
# Добавление в ZIP
# -------------------------------------------------------------------


def zip_add_stream(
    zout: zipfile.ZipFile,
    arcname: str,
    src_file,
    st_mode: Optional[int] = None,
) -> None:
    zi = zipfile.ZipInfo(arcname)
    zi.create_system = 3
    zi.date_time = ZIP_DT
    zi.compress_type = zout.compression
    zi.external_attr = normalized_zip_mode(arcname, st_mode) << 16
    with zout.open(zi, "w") as dst:
        shutil.copyfileobj(src_file, dst, length=1024 * 1024)


def open_zip_write(output_zip: Path) -> zipfile.ZipFile:
    """
    compresslevel поддерживается не во всех версиях Python (добавили в 3.7),
    поэтому пробуем и откатываемся.
    """
    try:
        return zipfile.ZipFile(
            output_zip,
            mode="w",
            compression=zipfile.ZIP_DEFLATED,
            compresslevel=1,
        )
    except TypeError:
        return zipfile.ZipFile(
            output_zip,
            mode="w",
            compression=zipfile.ZIP_DEFLATED,
        )


# -------------------------------------------------------------------
# Упаковка директории
# -------------------------------------------------------------------


def pack_dir_fast(
    input_dir: Path,
    output_zip: Path,
    max_bytes: int,
    allow_dotenv: bool,
    report_path: Optional[Path],
) -> None:
    input_stat = input_dir.lstat()
    if path_entry_is_link_or_reparse(input_dir, input_stat):
        raise ValueError("input directory must not be a symlink or reparse point")
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
            # режем мусорные директории и machine-local runtime path prefixes
            if path_has_junk_prefix(rel_posix):
                stats.add_drop("junk_path_prefix", rel=rel_posix)
                continue
            path_reason = generated_state_path_reason(rel_posix)
            if path_reason:
                stats.add_drop(path_reason, rel=rel_posix)
                continue
            if any(part in JUNK_DIR_NAMES for part in rel_posix.split("/")):
                stats.add_drop("junk_dir", rel=rel_posix)
                continue

            try:
                st = abs_path.lstat()
                resolved_path = abs_path.resolve(strict=True)
            except OSError:
                stats.add_drop("stat_failed", rel=rel_posix)
                continue
            if path_has_link_component(abs_path, root):
                stats.add_drop("symlink_component", rel=rel_posix)
                continue
            if not stat.S_ISREG(st.st_mode):
                stats.add_drop("non_regular_file", rel=rel_posix)
                continue
            if not path_is_within(resolved_path, root):
                stats.add_drop("outside_input_root", rel=rel_posix)
                continue
            size = st.st_size

            reason = drop_reason_for_file(
                rel_posix, size, max_bytes, allow_dotenv, dotenv_fallback_filter
            )
            if reason:
                stats.add_drop(reason, rel=rel_posix, size=size)
                continue

            try:
                with read_regular_file_stable(abs_path, root, st) as f:
                    zip_add_stream(
                        zout,
                        rel_posix,
                        f,
                        st_mode=getattr(st, "st_mode", None),
                    )
                stats.add_keep(size)
            except Exception as error:
                stats.add_drop(
                    "read_or_zip_failed",
                    rel=rel_posix,
                    size=size,
                    detail=str(error),
                )

    print(f"OK: {output_zip}")
    print(f"kept={stats.kept} dropped={stats.dropped}")
    print(f"kept_bytes={stats.kept_bytes} dropped_bytes={stats.dropped_bytes}")
    print(f"used_git={stats.used_git}")

    if report_path:
        report_path.parent.mkdir(parents=True, exist_ok=True)
        report_path.write_text(
            json.dumps(stats.to_jsonable(), ensure_ascii=False, indent=2),
            encoding="utf-8",
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
    input_zip: Path,
    output_zip: Path,
    max_bytes: int,
    allow_dotenv: bool,
    report_path: Optional[Path],
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
        prefix = detect_single_toplevel_prefix(
            i.filename.replace("\\", "/") for i in infos
        )

        # Детерминированный порядок: сортируем по будущему имени в архиве
        sortable: List[Tuple[str, zipfile.ZipInfo]] = []
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

            if path_has_junk_prefix(rel):
                stats.add_drop("junk_path_prefix", rel=rel, size=info.file_size or 0)
                continue
            path_reason = generated_state_path_reason(rel)
            if path_reason:
                stats.add_drop(path_reason, rel=rel, size=info.file_size or 0)
                continue
            if any(part in JUNK_DIR_NAMES for part in rel.split("/")):
                stats.add_drop("junk_dir", rel=rel, size=info.file_size or 0)
                continue

            size = info.file_size or 0

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
                    zi.create_system = 3
                    zi.date_time = ZIP_DT
                    zi.compress_type = zout.compression
                    zi.external_attr = (
                        normalized_zip_mode(rel, info.external_attr >> 16) << 16
                    )

                    with zout.open(zi, "w") as dst:
                        shutil.copyfileobj(f, dst, length=1024 * 1024)

                stats.add_keep(size)
            except Exception:
                stats.add_drop("read_or_zip_failed", rel=rel, size=size)

    print(f"OK: {output_zip}")
    print(f"kept={stats.kept} dropped={stats.dropped}")
    print(f"kept_bytes={stats.kept_bytes} dropped_bytes={stats.dropped_bytes}")

    if report_path:
        report_path.parent.mkdir(parents=True, exist_ok=True)
        report_path.write_text(
            json.dumps(stats.to_jsonable(), ensure_ascii=False, indent=2),
            encoding="utf-8",
        )
        print(f"report: {report_path}")


# -------------------------------------------------------------------
# CLI
# -------------------------------------------------------------------


def _try_parse_int(s: str) -> Optional[int]:
    try:
        return int(s)
    except Exception:
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


def parse_cli(
    argv: List[str],
) -> Tuple[Optional[str], Optional[str], int, Optional[str], bool, Optional[str]]:
    """
    Returns: (input_str, out_str, max_bytes, report_str, allow_dotenv, err)
    Keeps backward compatibility with positional [input] [output] [max_bytes].
    """
    if any(a in ("-h", "--help") for a in argv):
        print_help()
        return None, None, DEFAULT_MAX_FILE_BYTES, None, False, "EXIT_HELP"

    allow_dotenv = False
    report_str: Optional[str] = None
    max_bytes: Optional[int] = None

    args: List[str] = []
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

    return (
        None,
        None,
        DEFAULT_MAX_FILE_BYTES,
        report_str,
        allow_dotenv,
        "ERROR: too many arguments",
    )


def pick_input_auto(cwd: Path) -> Tuple[str, Path]:
    zips: List[Path] = []
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
    input_str, out_str, max_bytes, report_str, allow_dotenv, err = parse_cli(
        sys.argv[1:]
    )
    if err == "EXIT_HELP":
        return 0
    if err:
        print(err, file=sys.stderr)
        return 2

    cwd = Path.cwd()

    if input_str:
        src = Path(os.path.abspath(Path(input_str).expanduser()))
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
            root,
            final_out,
            max_bytes=max_bytes,
            allow_dotenv=allow_dotenv,
            report_path=report_path,
        )
        return 0

    final_out = out_zip if out_zip else (src.parent / make_output_name(src.stem))
    clean_zip_fast(
        src,
        final_out,
        max_bytes=max_bytes,
        allow_dotenv=allow_dotenv,
        report_path=report_path,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
