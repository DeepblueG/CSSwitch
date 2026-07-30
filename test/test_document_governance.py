"""Minimal repository-local Markdown governance checks.

These checks establish source-level documentation structure only. They do not
establish product, artifact, installed-runtime, live, signing, or release truth.
"""

from __future__ import annotations

import os
import re
import stat
import subprocess
import tempfile
import unicodedata
import unittest
from datetime import date
from pathlib import Path
from urllib.parse import unquote, urlsplit


REPO_ROOT = Path(__file__).resolve().parents[1]
MAX_MARKDOWN_BYTES = 4 * 1024 * 1024
DIRECTORY_FLAGS = os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NOFOLLOW
FILE_FLAGS = os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW
FENCE_RE = re.compile(r"^ {0,3}(`{3,}|~{3,})(.*)$")
QUOTE_RE = re.compile(r"^ {0,3}> ?")
LIST_RE = re.compile(
    r"^( {0,3})((?:[-+*]|\d{1,9}[.)]))([ \t]{1,4})(.*)$"
)
REFERENCE_RE = re.compile(r"^\s{0,3}\[[^\]]+\]:\s*(<[^>]+>|\S+)")
ATX_HEADING_RE = re.compile(
    r"^\s{0,3}(#{1,6})\s+(.+?)\s*#*\s*$",
    re.MULTILINE,
)
HEADING_LINE_RE = re.compile(r"^\s{0,3}#{1,6}\s+(.+?)\s*#*\s*$")
HTML_ID_RE = re.compile(
    r"<(?:a|[A-Za-z][A-Za-z0-9-]*)\b[^>]*\bid=[\"']([^\"']+)[\"']",
    re.IGNORECASE,
)
HTML_COMMENT_RE = re.compile(r"<!--.*?-->", re.DOTALL)
HTML_TAG_RE = re.compile(r"<[^>]+>")
INLINE_CODE_RE = re.compile(r"`+([^`]*)`+")
MARKDOWN_LINK_TEXT_RE = re.compile(r"!?\[([^\]]*)\]\([^)]*\)")
METADATA_RE = re.compile(r"^([^：\s][^：]{0,31})：\s*(.*)$")
PLACEHOLDER_RE = re.compile(
    r"^(?:todo|tbd|待定|待补|待填写|待确认)"
    r"(?:$|[\s:：.,。;；!?！？（(])",
    re.IGNORECASE,
)


def relative_parts(path: Path, root: Path) -> tuple[str, ...]:
    candidate = Path(os.path.normpath(str(path.absolute())))
    try:
        relative = candidate.relative_to(root.absolute())
    except ValueError as exc:
        raise AssertionError(f"path escapes repository: {path}") from exc
    if not relative.parts or any(part in {"", ".", ".."} for part in relative.parts):
        raise AssertionError(f"invalid repository path: {path}")
    return relative.parts


def open_parent(path: Path, root: Path) -> tuple[list[int], str]:
    parts = relative_parts(path, root)
    descriptors: list[int] = []
    try:
        current = os.open(root, DIRECTORY_FLAGS)
        descriptors.append(current)
        for part in parts[:-1]:
            current = os.open(part, DIRECTORY_FLAGS, dir_fd=current)
            descriptors.append(current)
    except OSError as exc:
        for descriptor in reversed(descriptors):
            os.close(descriptor)
        raise AssertionError(f"unsafe repository path: {path}") from exc
    return descriptors, parts[-1]


def entry_type(path: Path, root: Path = REPO_ROOT) -> str:
    descriptors, name = open_parent(path, root)
    try:
        try:
            metadata = os.stat(
                name,
                dir_fd=descriptors[-1],
                follow_symlinks=False,
            )
        except FileNotFoundError:
            return "missing"
        if stat.S_ISLNK(metadata.st_mode):
            raise AssertionError(f"repository path is a symlink: {path}")
        if stat.S_ISREG(metadata.st_mode):
            return "file"
        if stat.S_ISDIR(metadata.st_mode):
            return "directory"
        raise AssertionError(f"unsupported repository path type: {path}")
    finally:
        for descriptor in reversed(descriptors):
            os.close(descriptor)


def read_text(path: Path, root: Path = REPO_ROOT) -> str | None:
    descriptors, name = open_parent(path, root)
    file_descriptor: int | None = None
    try:
        try:
            file_descriptor = os.open(name, FILE_FLAGS, dir_fd=descriptors[-1])
        except FileNotFoundError:
            return None
        except OSError as exc:
            raise AssertionError(f"unsafe repository file: {path}") from exc
        metadata = os.fstat(file_descriptor)
        if not stat.S_ISREG(metadata.st_mode):
            raise AssertionError(f"repository file is not regular: {path}")
        if metadata.st_size > MAX_MARKDOWN_BYTES:
            raise AssertionError(f"Markdown exceeds size limit: {path}")
        chunks: list[bytes] = []
        remaining = MAX_MARKDOWN_BYTES + 1
        while remaining:
            chunk = os.read(file_descriptor, min(65536, remaining))
            if not chunk:
                break
            chunks.append(chunk)
            remaining -= len(chunk)
        raw = b"".join(chunks)
        if len(raw) > MAX_MARKDOWN_BYTES:
            raise AssertionError(f"Markdown exceeds size limit: {path}")
        return raw.decode("utf-8", "strict")
    finally:
        if file_descriptor is not None:
            os.close(file_descriptor)
        for descriptor in reversed(descriptors):
            os.close(descriptor)


def maintained_markdown(root: Path = REPO_ROOT) -> dict[Path, str]:
    result = subprocess.run(
        [
            "git",
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            "*.md",
        ],
        cwd=root,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    contents: dict[Path, str] = {}
    for raw_name in result.stdout.split(b"\0"):
        if not raw_name:
            continue
        path = root / os.fsdecode(raw_name)
        kind = entry_type(path, root)
        if kind == "missing":
            continue
        if kind != "file":
            raise AssertionError(f"maintained Markdown is not a file: {path}")
        content = read_text(path, root)
        if content is None:
            raise AssertionError(f"maintained Markdown disappeared: {path}")
        contents[path] = content
    return dict(
        sorted(
            contents.items(),
            key=lambda item: item[0].relative_to(root).as_posix(),
        )
    )


def visual_columns(value: str) -> int:
    columns = 0
    for character in value:
        columns += 4 - columns % 4 if character == "\t" else 1
    return columns


def strip_indent(line: str, columns: int) -> str | None:
    consumed = 0
    index = 0
    while index < len(line) and line[index] in {" ", "\t"}:
        character = line[index]
        consumed += 4 - consumed % 4 if character == "\t" else 1
        index += 1
        if consumed >= columns:
            return " " * (consumed - columns) + line[index:]
    return None


Container = tuple[str, int]


def container_openers(line: str) -> tuple[tuple[Container, ...], str]:
    containers: list[Container] = []
    current = line
    while True:
        quote = QUOTE_RE.match(current)
        if quote is not None:
            containers.append(("quote", 0))
            current = current[quote.end():]
            continue
        listed = LIST_RE.match(current)
        if listed is not None:
            prefix = "".join(listed.group(index) for index in (1, 2, 3))
            containers.append(("list", visual_columns(prefix)))
            current = listed.group(4)
            continue
        return tuple(containers), current


def strip_container_continuation(
    line: str,
    containers: tuple[Container, ...],
) -> str | None:
    current = line
    for kind, width in containers:
        if kind == "quote":
            match = QUOTE_RE.match(current)
            if match is None:
                return None
            current = current[match.end():]
        else:
            if not current.strip():
                return ""
            current = strip_indent(current, width)
            if current is None:
                return None
    return current


def without_code_blocks(text: str) -> str:
    visible: list[str] = []
    fence_character = ""
    fence_length = 0
    fence_containers: tuple[Container, ...] = ()
    for line in text.splitlines(keepends=True):
        raw = line.rstrip("\r\n")
        if fence_character:
            scoped = strip_container_continuation(raw, fence_containers)
            if scoped is None:
                fence_character = ""
                fence_length = 0
                fence_containers = ()
            else:
                close = re.fullmatch(
                    rf" {{0,3}}{re.escape(fence_character)}"
                    rf"{{{fence_length},}}\s*",
                    scoped,
                )
                if close is not None:
                    fence_character = ""
                    fence_length = 0
                    fence_containers = ()
                visible.append("\n" if line.endswith(("\n", "\r")) else "")
                continue
        containers, content = container_openers(raw)
        opening = FENCE_RE.match(content)
        if opening is not None:
            fence = opening.group(1)
            if fence[0] == "~" or "`" not in opening.group(2):
                fence_character = fence[0]
                fence_length = len(fence)
                fence_containers = containers
                visible.append("\n" if line.endswith(("\n", "\r")) else "")
                continue
        if content.startswith(("    ", "\t")):
            visible.append("\n" if line.endswith(("\n", "\r")) else "")
            continue
        visible.append(line)
    return "".join(visible)


def escaped_at(text: str, index: int) -> bool:
    backslashes = 0
    cursor = index - 1
    while cursor >= 0 and text[cursor] == "\\":
        backslashes += 1
        cursor -= 1
    return backslashes % 2 == 1


def run_length(text: str, start: int, character: str) -> int:
    end = start
    while end < len(text) and text[end] == character:
        end += 1
    return end - start


def without_inline_code(text: str) -> str:
    visible: list[str] = []
    index = 0
    while index < len(text):
        opening = text.find("`", index)
        if opening < 0:
            visible.append(text[index:])
            break
        length = run_length(text, opening, "`")
        if escaped_at(text, opening):
            visible.append(text[index:opening + length])
            index = opening + length
            continue
        cursor = opening + length
        closing: int | None = None
        while cursor < len(text):
            candidate = text.find("`", cursor)
            if candidate < 0:
                break
            candidate_length = run_length(text, candidate, "`")
            if not escaped_at(text, candidate) and candidate_length == length:
                closing = candidate
                break
            cursor = candidate + candidate_length
        if closing is None:
            visible.append(text[index:opening + length])
            index = opening + length
            continue
        end = closing + length
        visible.append(text[index:opening])
        visible.append(
            "".join(
                character if character in {"\r", "\n"} else " "
                for character in text[opening:end]
            )
        )
        index = end
    return "".join(visible)


def matching_delimiter(
    text: str,
    start: int,
    opening: str,
    closing: str,
) -> int | None:
    depth = 0
    index = start
    while index < len(text):
        if text[index] == "\\":
            index += 2
            continue
        if text[index] == opening:
            depth += 1
        elif text[index] == closing:
            depth -= 1
            if depth == 0:
                return index
        index += 1
    return None


def inline_targets(text: str) -> tuple[str, ...]:
    targets: list[str] = []
    index = 0
    while index < len(text):
        label_start = text.find("[", index)
        if label_start < 0:
            break
        if escaped_at(text, label_start):
            index = label_start + 1
            continue
        label_end = matching_delimiter(text, label_start, "[", "]")
        if (
            label_end is None
            or label_end + 1 >= len(text)
            or text[label_end + 1] != "("
        ):
            index = label_start + 1
            continue
        cursor = label_end + 2
        while cursor < len(text) and text[cursor] in " \t\r\n":
            cursor += 1
        if cursor < len(text) and text[cursor] == "<":
            end = cursor + 1
            while end < len(text) and text[end] != ">":
                end += 2 if text[end] == "\\" else 1
            if end < len(text):
                targets.append(text[cursor + 1:end])
                index = end + 1
                continue
            index = label_end + 1
            continue
        start = cursor
        depth = 0
        while cursor < len(text):
            character = text[cursor]
            if character == "\\":
                cursor += 2
                continue
            if character == "(":
                depth += 1
            elif character == ")":
                if depth == 0:
                    targets.append(text[start:cursor])
                    index = cursor + 1
                    break
                depth -= 1
            elif character in " \t\r\n" and depth == 0:
                targets.append(text[start:cursor])
                closing = matching_delimiter(text, label_end + 1, "(", ")")
                index = closing + 1 if closing is not None else label_end + 1
                break
            cursor += 1
        else:
            index = label_end + 1
    return tuple(targets)


def link_targets(text: str) -> tuple[str, ...]:
    body = without_inline_code(HTML_COMMENT_RE.sub("", without_code_blocks(text)))
    targets = list(inline_targets(body))
    for line in body.splitlines():
        _, content = container_openers(line)
        match = REFERENCE_RE.match(content)
        if match is not None:
            targets.append(match.group(1))
    return tuple(target.strip("<>") for target in targets)


def github_slug(value: str) -> str:
    value = MARKDOWN_LINK_TEXT_RE.sub(r"\1", value)
    value = INLINE_CODE_RE.sub(r"\1", value)
    value = HTML_TAG_RE.sub("", value).strip().lower()
    value = "".join(
        character
        for character in value
        if character in {" ", "-", "_"}
        or not unicodedata.category(character).startswith(("P", "S", "C"))
    )
    return re.sub(r"\s+", "-", value)


def heading_anchors(text: str) -> set[str]:
    visible = HTML_COMMENT_RE.sub("", without_code_blocks(text))
    html_visible = without_inline_code(visible)
    anchors = {unquote(match.group(1)) for match in HTML_ID_RE.finditer(html_visible)}
    duplicates: dict[str, int] = {}
    for match in ATX_HEADING_RE.finditer(visible):
        base = github_slug(match.group(2))
        if not base:
            continue
        duplicate = duplicates.get(base, 0)
        anchors.add(base if duplicate == 0 else f"{base}-{duplicate}")
        duplicates[base] = duplicate + 1
    return anchors


def metadata(text: str) -> dict[str, str]:
    lines = without_code_blocks(text).splitlines()
    first = next((index for index, line in enumerate(lines) if line.strip()), None)
    if first is None or not re.match(r"^\s{0,3}#\s+\S", lines[first]):
        return {}
    fields: dict[str, str] = {}
    for line in lines[first + 1:]:
        if not line.strip():
            continue
        match = METADATA_RE.match(line.strip())
        if match is None:
            break
        fields[match.group(1).strip()] = match.group(2).strip()
    return fields


def visible_value(value: str | None) -> str:
    if value is None:
        return ""
    result = HTML_COMMENT_RE.sub("", value).strip()
    _, result = container_openers(result)
    result = MARKDOWN_LINK_TEXT_RE.sub(r"\1", result)
    result = re.sub(r"^\[[ xX]\]\s*", "", result)
    return result.strip(" \t\r\n`*_~-")


def meaningful(value: str | None) -> bool:
    result = visible_value(value)
    if not result:
        return False
    if result.lower() in {"n/a", "na", "never", "none", "null", "永久", "无"}:
        return False
    return PLACEHOLDER_RE.match(result) is None


def review_date(value: str | None) -> bool:
    if value is None:
        return False
    match = re.match(r"^(\d{4}-\d{2}-\d{2})(?:$|[\s（(。])", value.strip())
    if match is None:
        return False
    try:
        date.fromisoformat(match.group(1))
    except ValueError:
        return False
    return True


def section_content(text: str, names: tuple[str, ...]) -> bool:
    fields = metadata(text)
    if any(meaningful(fields.get(name)) for name in names):
        return True
    lines = HTML_COMMENT_RE.sub("", without_code_blocks(text)).splitlines()
    for index, line in enumerate(lines):
        match = HEADING_LINE_RE.match(line)
        if match is None or match.group(1).strip() not in names:
            continue
        for child in lines[index + 1:]:
            if HEADING_LINE_RE.match(child):
                break
            if meaningful(child):
                return True
    return False


def compatibility_pointer(text: str) -> bool:
    return metadata(text).get("状态") == "兼容指针"


def resolve_target(source: Path, raw: str) -> tuple[Path, str] | None:
    target = raw.replace(r"\(", "(").replace(r"\)", ")")
    parsed = urlsplit(target)
    if parsed.scheme or parsed.netloc or target.startswith(("/", "//")):
        return None
    candidate = source if not parsed.path else source.parent / unquote(parsed.path)
    normalized = Path(os.path.normpath(str(candidate.absolute())))
    relative_parts(normalized, REPO_ROOT)
    return normalized, unquote(parsed.fragment)


def markdown_edges(contents: dict[Path, str]) -> dict[Path, set[Path]]:
    maintained = set(contents)
    edges = {path: set() for path in contents}
    for source, text in contents.items():
        for raw in link_targets(text):
            resolved = resolve_target(source, raw)
            if resolved is None:
                continue
            target, _ = resolved
            if entry_type(target) == "directory":
                target = target / "README.md"
            if target.suffix.lower() == ".md" and target in maintained:
                edges[source].add(target)
    return edges


class DocumentGovernanceTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contents = maintained_markdown()
        cls.paths = tuple(cls.contents)
        cls.path_set = set(cls.paths)
        cls.edges = markdown_edges(cls.contents)

    def test_markdown_relative_links_and_anchors_resolve(self) -> None:
        for example in (
            "````md\n[x](missing.md)\n````\n",
            "- > ```md\n  > [x](missing.md)\n  > ```\n",
            "`code\n[x](missing.md)\nmore`\n",
        ):
            self.assertEqual(link_targets(example), ())
        self.assertEqual(
            link_targets("- > [ref]: missing.md\n"),
            ("missing.md",),
        )
        self.assertEqual(link_targets(r"\[x](missing.md)"), ())
        self.assertEqual(link_targets(r"\\[x](missing.md)"), ("missing.md",))
        self.assertEqual(link_targets("[x](foo(bar).md)"), ("foo(bar).md",))
        self.assertNotIn(
            "ghost",
            heading_anchors("```html\n<a id='ghost'></a>\n```\n"),
        )
        with tempfile.TemporaryDirectory(prefix="csswitch-doc-git-") as raw:
            root = Path(raw)
            subprocess.run(
                ["git", "init", "-q"],
                cwd=root,
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            unicode_path = root / "中文.md"
            newline_path = root / "line\nbreak.md"
            unicode_path.write_text("[x](missing.md)\n", encoding="utf-8")
            newline_path.write_text("# newline\n", encoding="utf-8")
            subprocess.run(
                ["git", "add", "--", unicode_path.name],
                cwd=root,
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            isolated = maintained_markdown(root)
            self.assertIn(unicode_path, isolated)
            self.assertIn(newline_path, isolated)
        errors: list[str] = []
        anchors: dict[Path, set[str]] = {}
        for source, text in self.contents.items():
            display = source.relative_to(REPO_ROOT).as_posix()
            for raw in link_targets(text):
                try:
                    resolved = resolve_target(source, raw)
                except AssertionError:
                    errors.append(f"{display}: target escapes repository {raw}")
                    continue
                if resolved is None:
                    continue
                target, fragment = resolved
                try:
                    kind = entry_type(target)
                except AssertionError:
                    errors.append(f"{display}: unsafe target {raw}")
                    continue
                if kind == "missing":
                    errors.append(f"{display}: missing target {raw}")
                    continue
                if kind == "directory":
                    if fragment:
                        errors.append(f"{display}: directory target has anchor {raw}")
                    continue
                if fragment and target.suffix.lower() == ".md":
                    if target not in self.contents:
                        errors.append(f"{display}: unmaintained Markdown target {raw}")
                        continue
                    available = anchors.setdefault(
                        target,
                        heading_anchors(self.contents[target]),
                    )
                    if fragment not in available:
                        errors.append(f"{display}: missing anchor {raw}")
        self.assertEqual(errors, [])

    def test_agent_rule_index_reaches_every_rule(self) -> None:
        root = REPO_ROOT / "AGENTS.md"
        index = REPO_ROOT / ".agents" / "rules" / "README.md"
        self.assertIn(index, self.edges.get(root, set()))
        required = {
            path for path in self.paths if path.parent == index.parent and path != index
        }
        missing = required - self.edges.get(index, set())
        self.assertEqual(
            [path.relative_to(REPO_ROOT).as_posix() for path in sorted(missing)],
            [],
        )

    def test_docs_index_reaches_current_maintained_bodies(self) -> None:
        root = REPO_ROOT / "docs" / "README.md"
        categories = tuple(
            REPO_ROOT / "docs" / name
            for name in ("architecture", "features", "operations")
        )
        required_roots = {
            *(category / "README.md" for category in categories),
            REPO_ROOT / "docs" / "evidence" / "README.md",
            REPO_ROOT / "docs" / "references" / "README.md",
        }
        self.assertEqual(
            [
                path.relative_to(REPO_ROOT).as_posix()
                for path in sorted(required_roots - self.edges.get(root, set()))
            ],
            [],
        )
        missing: list[str] = []
        for category in categories:
            nested_indexes = {
                path
                for path in self.paths
                if category in path.parents
                and path.name == "README.md"
                and path != category / "README.md"
            }
            for nested in nested_indexes:
                directory = nested.parent.parent
                while directory != category and (
                    directory / "README.md"
                ) not in self.path_set:
                    directory = directory.parent
                parent = directory / "README.md"
                if nested not in self.edges.get(parent, set()):
                    missing.append(
                        f"{parent.relative_to(REPO_ROOT)} -> "
                        f"{nested.relative_to(REPO_ROOT)}"
                    )
            bodies = {
                path
                for path in self.paths
                if category in path.parents
                and path.name != "README.md"
                and not compatibility_pointer(self.contents[path])
            }
            for body in bodies:
                directory = body.parent
                while directory != category and (
                    directory / "README.md"
                ) not in self.path_set:
                    directory = directory.parent
                index = directory / "README.md"
                if body not in self.edges.get(index, set()):
                    missing.append(
                        f"{index.relative_to(REPO_ROOT)} -> "
                        f"{body.relative_to(REPO_ROOT)}"
                    )
        explicit = {
            REPO_ROOT / "docs" / "evidence" / "README.md": {
                REPO_ROOT / "docs" / "evidence" / "investigations" / "README.md",
                REPO_ROOT / "docs" / "evidence" / "releases" / "README.md",
            },
            REPO_ROOT / "docs" / "references" / "README.md": {
                REPO_ROOT / "docs" / "references" / "external" / "README.md",
            },
            REPO_ROOT / "docs" / "references" / "external" / "README.md": {
                REPO_ROOT / "docs" / "references" / "external" / "csnative.md",
            },
        }
        for index, targets in explicit.items():
            for target in targets & self.path_set:
                if target not in self.edges.get(index, set()):
                    missing.append(
                        f"{index.relative_to(REPO_ROOT)} -> "
                        f"{target.relative_to(REPO_ROOT)}"
                    )
        self.assertEqual(sorted(missing), [])

    def test_lifecycle_metadata_is_present(self) -> None:
        for placeholder in (
            "TODO",
            "TBD.",
            "待定。",
            "TODO（稍后补）",
            "[TODO](later.md)",
            "> TODO",
        ):
            self.assertFalse(meaningful(placeholder))
            self.assertFalse(
                section_content(
                    f"# Draft\n\n## 决策范围\n\n{placeholder}\n",
                    ("决策范围",),
                )
            )
        errors: list[str] = []
        context_dir = REPO_ROOT / ".agents" / "context"
        for path, text in self.contents.items():
            relative = path.relative_to(REPO_ROOT).as_posix()
            fields = metadata(text)
            if path.parent == context_dir and path.name != "README.md":
                if not review_date(fields.get("最后复核")) or not meaningful(
                    fields.get("失效条件")
                ):
                    errors.append(f"{relative}: context metadata")
            if compatibility_pointer(text):
                declared = link_targets(fields.get("当前权威入口", ""))
                authorities: set[Path] = set()
                invalid = False
                for raw in declared:
                    try:
                        resolved = resolve_target(path, raw)
                    except AssertionError:
                        invalid = True
                        continue
                    if resolved is None:
                        invalid = True
                        continue
                    target, fragment = resolved
                    if entry_type(target) == "directory":
                        target = target / "README.md"
                    if (
                        target not in self.contents
                        or compatibility_pointer(self.contents[target])
                        or (
                            fragment
                            and fragment not in heading_anchors(self.contents[target])
                        )
                    ):
                        invalid = True
                        continue
                    authorities.add(target)
                if (
                    not meaningful(fields.get("失效条件"))
                    or not authorities
                    or invalid
                ):
                    errors.append(
                        f"{relative}: compatibility-pointer metadata/authority"
                    )
            if relative.startswith(".agents/handoffs/") and path.name != "README.md":
                if not review_date(fields.get("最后更新")) or not meaningful(
                    fields.get("失效条件")
                ):
                    errors.append(f"{relative}: handoff metadata")
                if path.name.endswith(".plan.md") and not all(
                    section_content(text, (name,))
                    for name in ("目标", "范围", "当前 checkpoint")
                ):
                    errors.append(f"{relative}: plan content")
            if relative.startswith("docs/plans/"):
                errors.append(
                    f"{relative}: Plan must use .agents/handoffs/*.plan.md"
                )
            if (
                relative.startswith("docs/drafts/")
                and relative != "docs/drafts/README.md"
            ):
                complete = (
                    section_content(text, ("决策范围",))
                    and section_content(text, ("未决项",))
                    and section_content(text, ("接受者", "接受条件"))
                )
                if (
                    fields.get("状态") not in {"草拟", "待评审"}
                    or not review_date(fields.get("最后有效评审"))
                    or not meaningful(fields.get("失效条件"))
                    or not complete
                ):
                    errors.append(f"{relative}: draft metadata/content")
        self.assertEqual(errors, [])


if __name__ == "__main__":
    unittest.main()
