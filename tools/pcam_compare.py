#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Compare Phantom Camera parity recordings (JSONL) frame by frame.

The Phantom Camera port is verified by recording the *same* demo scene twice —
once on the GDScript oracle (`tools/pcam_parity.sh --baseline`, goldens under
`parity/pcam_golden/`) and once on the Rust GDExtension
(`--candidate`, under `client/rust/target/pcam/`) — and diffing the observable
frame pipeline: the real `Camera2D`/`Camera3D` transform, the active Phantom
Camera's `get_transform_output()`, `is_active()`, `get_priority()`,
`get_noise_transform()`, `has_noise_resource()`, and the emitted signal stream.

Both recordings are written by the recorder node
(`client/rust/phantom-camera-rust/src/dev/recorder.rs`, schema
`pcam-record/v1`): line 1 is a header, every following line is one frame.

Exit codes follow `parity/AGENTS.md`: `0` every compared scene matches, `1` at
least one mismatch, `2` usage or IO problem (missing/unparsable file, no scene
pairs, bad `--tolerance`).

Channels and their tolerances are the constants below; override one per run with
`--tolerance <channel>=<value>` (e.g. `--tolerance pos=1e-3`).
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

# --- tolerances (absolute, per channel) -------------------------------------
# Every value is the max |golden - candidate| allowed for that channel. Raise one
# only together with a note in the report explaining why the oracle and the port
# may legitimately disagree at that scale.

POSITION_TOLERANCE = 1e-4        # `pos` — camera global transform origin
ROTATION_TOLERANCE = 1e-4        # `rot` — 2D angle / 3D quaternion
ZOOM_TOLERANCE = 1e-4            # `zoom` — zoom, fov, size, near/far, offsets
TRANSFORM_TOLERANCE = 1e-4       # `transform` — pcam get_transform_output()
NOISE_TOLERANCE = 1e-4           # `noise` — pcam get_noise_transform()
ACTIVE_TOLERANCE = 0.0           # `active` — pcam is_active(), exact
PRIORITY_TOLERANCE = 0.0         # `priority` — pcam get_priority(), exact
NOISE_FLAG_TOLERANCE = 0.0       # `has_noise` — pcam has_noise_resource(), exact
SIGNALS_TOLERANCE = 0.0          # `signals` — exact `name@frame` sequence
DELTA_TOLERANCE = 1e-6           # `d` — recorded frame delta
HEADER_TOLERANCE = 0.0           # `header` — schema, dims, camera/pcam class names
FRAMES_TOLERANCE = 0.0           # `frames` — frame count and per-index presence

#: Printed channel order; also the `--tolerance` keys (except `frames`, which is
#: structural and cannot be relaxed).
CHANNELS = (
    "pos",
    "rot",
    "zoom",
    "transform",
    "noise",
    "active",
    "priority",
    "has_noise",
    "signals",
    "d",
    "header",
)

DEFAULT_TOLERANCES: dict[str, float] = {
    "pos": POSITION_TOLERANCE,
    "rot": ROTATION_TOLERANCE,
    "zoom": ZOOM_TOLERANCE,
    "transform": TRANSFORM_TOLERANCE,
    "noise": NOISE_TOLERANCE,
    "active": ACTIVE_TOLERANCE,
    "priority": PRIORITY_TOLERANCE,
    "has_noise": NOISE_FLAG_TOLERANCE,
    "signals": SIGNALS_TOLERANCE,
    "d": DELTA_TOLERANCE,
    "header": HEADER_TOLERANCE,
    "frames": FRAMES_TOLERANCE,
}

# Scenes whose source data is nondeterministic, so the two legs cannot agree no
# matter the tolerance. Reported as SKIP with the reason instead of weakening a
# tolerance for every scene.
#
# Both noise scenes qualify for the same upstream reason: `PhantomCameraNoise{2,3}D._init`
# seeds `FastNoiseLite` with `randi()` (`phantom_camera_noise_2d.gd:88-90`,
# `phantom_camera_noise_3d.gd:88-90`), and the scene's `randomize_noise_seed = false`
# is applied *after* `_init`, so the seed is random on every run of either leg —
# `noise_seed` only shifts the sample's X coordinate. Verified by recording the
# oracle twice in an isolated project: consecutive runs differ while
# `pos`/`transform` stay bit-identical. The port reproduces the resource faithfully
# (`resources.rs`), so this is an oracle property, not a port divergence.
NONDETERMINISTIC_SCENES: dict[str, str] = {
    "2d_noise_example_scene": (
        "skipped: PhantomCameraNoise2D reseeds FastNoiseLite with randi() in _init, "
        "before the scene applies randomize_noise_seed=false, so the noise waveform "
        "differs on every run of either leg."
    ),
    "3d_noise_example_scene": (
        "skipped: PhantomCameraNoise3D reseeds FastNoiseLite with randi() in _init, "
        "before the scene applies randomize_noise_seed=false, so the noise waveform "
        "differs on every run of either leg."
    ),
}

#: Channels compared as numbers; everything else is compared exactly.
NUMERIC_CHANNELS = frozenset({"pos", "rot", "zoom", "transform", "noise", "d"})

SCHEMA = "pcam-record/v1"
EXIT_PASS = 0
EXIT_MISMATCH = 1
EXIT_USAGE = 2


class UsageError(Exception):
    """Bad arguments or unusable input (exit code 2)."""


@dataclass
class Header:
    schema: str
    observer: str
    dims: int | None
    camera_class: str
    pcam_class: str
    host_class: str
    frames: int | None
    godot: str


@dataclass
class Recording:
    path: Path
    header: Header
    frames: dict[int, dict[str, Any]]


@dataclass
class Mismatch:
    frame: int
    channel: str
    golden: Any
    candidate: Any

    def as_dict(self) -> dict[str, Any]:
        return {
            "frame": self.frame,
            "channel": self.channel,
            "golden": self.golden,
            "candidate": self.candidate,
        }


def empty_counters() -> dict[str, int]:
    return {channel: 0 for channel in CHANNELS + ("frames",)}


def empty_errors() -> dict[str, float]:
    return {channel: 0.0 for channel in NUMERIC_CHANNELS}


@dataclass
class SceneResult:
    name: str
    status: str = "SKIP"
    reason: str = ""
    golden_path: str = ""
    candidate_path: str = ""
    golden_frames: int = 0
    candidate_frames: int = 0
    golden_header: Header | None = None
    candidate_header: Header | None = None
    mismatches: list[Mismatch] = field(default_factory=list)
    mismatch_count: int = 0
    channel_mismatches: dict[str, int] = field(default_factory=empty_counters)
    max_error: dict[str, float] = field(default_factory=empty_errors)

    def as_dict(self) -> dict[str, Any]:
        return {
            "scene": self.name,
            "status": self.status,
            "reason": self.reason,
            "golden": self.golden_path,
            "candidate": self.candidate_path,
            "golden_frames": self.golden_frames,
            "candidate_frames": self.candidate_frames,
            "golden_header": self.golden_header.__dict__ if self.golden_header else None,
            "candidate_header": self.candidate_header.__dict__ if self.candidate_header else None,
            "mismatch_count": self.mismatch_count,
            "channel_mismatches": self.channel_mismatches,
            "max_error": self.max_error,
            "mismatches": [entry.as_dict() for entry in self.mismatches],
        }


# --- loading -----------------------------------------------------------------


def load_recording(path: Path) -> Recording:
    """Parse one JSONL recording; raises UsageError on anything unexpected."""
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as error:
        raise UsageError(f"cannot read {path}: {error}") from error

    lines = [line for line in text.splitlines() if line.strip()]
    if not lines:
        raise UsageError(f"{path}: empty recording")

    header = parse_header(path, lines[0])
    frames: dict[int, dict[str, Any]] = {}
    for number, line in enumerate(lines[1:], start=2):
        try:
            frame = json.loads(line)
        except json.JSONDecodeError as error:
            raise UsageError(f"{path}:{number}: invalid JSON ({error})") from error
        if not isinstance(frame, dict) or "f" not in frame:
            raise UsageError(f"{path}:{number}: not a frame object")
        try:
            index = int(frame["f"])
        except (TypeError, ValueError) as error:
            raise UsageError(f"{path}:{number}: frame index is not an integer") from error
        frames[index] = frame

    return Recording(path=path, header=header, frames=frames)


def parse_header(path: Path, line: str) -> Header:
    try:
        header = json.loads(line)
    except json.JSONDecodeError as error:
        raise UsageError(f"{path}:1: invalid JSON header ({error})") from error
    if not isinstance(header, dict) or header.get("kind") != "header":
        raise UsageError(f"{path}:1: missing pcam-record header line")

    schema = str(header.get("schema", ""))
    if schema != SCHEMA:
        raise UsageError(f"{path}:1: schema {schema!r}, expected {SCHEMA!r}")

    return Header(
        schema=schema,
        observer=str(header.get("observer", "?")),
        dims=header.get("dims") if isinstance(header.get("dims"), int) else None,
        camera_class=str(header.get("camera_class", "?")),
        pcam_class=str(header.get("pcam_class", "?")),
        host_class=str(header.get("host_class", "?")),
        frames=header.get("frames") if isinstance(header.get("frames"), int) else None,
        godot=str(header.get("godot", "?")),
    )


def discover_scenes(directory: Path) -> dict[str, Path]:
    """`{scene name: jsonl path}` for one recording directory."""
    if not directory.is_dir():
        return {}
    return {path.stem: path for path in sorted(directory.glob("*.jsonl"))}


# --- comparison --------------------------------------------------------------


def compare_numbers(golden: Any, candidate: Any, tolerance: float) -> tuple[bool, float]:
    """Compare scalar/list numeric channel values; returns `(matches, max_abs_error)`."""
    golden_values = as_number_list(golden)
    candidate_values = as_number_list(candidate)

    if golden_values is None or candidate_values is None:
        return (golden == candidate, 0.0)

    if len(golden_values) != len(candidate_values):
        return (False, math.inf)

    matches = True
    worst = 0.0
    for left, right in zip(golden_values, candidate_values):
        if left is None or right is None:
            if left is not right:
                matches = False
            continue
        difference = abs(left - right)
        worst = max(worst, difference)
        if difference > tolerance:
            matches = False

    return (matches, worst)


def as_number_list(value: Any) -> list[float | None] | None:
    """`[1, 2.0]`-shaped value as floats (`None` element = JSON null)."""
    if value is None:
        return None
    if isinstance(value, bool):
        return None
    if isinstance(value, (int, float)):
        return [float(value)]
    if isinstance(value, list):
        numbers: list[float | None] = []
        for entry in value:
            if entry is None:
                numbers.append(None)
            elif isinstance(entry, bool):
                return None
            elif isinstance(entry, (int, float)):
                numbers.append(float(entry))
            else:
                return None
        return numbers
    return None


def channel_value(frame: dict[str, Any], channel: str) -> Any:
    """Read a channel out of a frame line (the `pcam` fields are nested)."""
    if channel in {"transform", "noise", "active", "priority", "has_noise"}:
        pcam = frame.get("pcam")
        if not isinstance(pcam, dict):
            return None
        return pcam.get(channel)
    return frame.get(channel)


def add_mismatch(
    result: SceneResult,
    frame: int,
    channel: str,
    golden: Any,
    candidate: Any,
    limit: int,
) -> None:
    result.mismatch_count += 1
    result.channel_mismatches[channel] = result.channel_mismatches.get(channel, 0) + 1
    if len(result.mismatches) < limit:
        result.mismatches.append(
            Mismatch(frame=frame, channel=channel, golden=golden, candidate=candidate)
        )


def compare_headers(result: SceneResult, golden: Header, candidate: Header, limit: int) -> None:
    """Header evidence: schema, dimension and the Phantom Camera class that proves
    which backend ran.

    `host_class` is deliberately *not* compared: the oracle's host is an anonymous
    scripted node that reports `Node`/`Node2D`, while the port registers a native
    `PhantomCameraHost`. It stays in the report as provenance.
    """
    checks = (
        ("header.schema", golden.schema, candidate.schema),
        ("header.dims", golden.dims, candidate.dims),
        ("header.camera_class", golden.camera_class, candidate.camera_class),
        ("header.pcam_class", golden.pcam_class, candidate.pcam_class),
    )
    for channel, left, right in checks:
        if left != right:
            add_mismatch(result, 0, channel, left, right, limit)


def compare_scene(
    name: str,
    golden_path: Path | None,
    candidate_path: Path | None,
    tolerances: dict[str, float],
    limit: int,
) -> SceneResult:
    result = SceneResult(name=name)

    if name in NONDETERMINISTIC_SCENES:
        # Excluded rather than loosened: this scene's *source data* is random on
        # both legs, so no tolerance can make the recording comparable.
        result.status = "SKIP"
        result.reason = NONDETERMINISTIC_SCENES[name]
        result.golden_path = str(golden_path) if golden_path else ""
        result.candidate_path = str(candidate_path) if candidate_path else ""
        return result
    if golden_path is None:
        result.status = "SKIP"
        result.reason = "no golden recording (run --baseline first)"
        result.candidate_path = str(candidate_path) if candidate_path else ""
        return result
    if candidate_path is None:
        result.status = "SKIP"
        result.reason = "scene not recorded for the candidate"
        result.golden_path = str(golden_path)
        return result

    result.golden_path = str(golden_path)
    result.candidate_path = str(candidate_path)

    golden = load_recording(golden_path)
    candidate = load_recording(candidate_path)
    result.golden_header = golden.header
    result.candidate_header = candidate.header
    result.golden_frames = len(golden.frames)
    result.candidate_frames = len(candidate.frames)

    if result.golden_frames != result.candidate_frames:
        add_mismatch(result, 0, "frames", result.golden_frames, result.candidate_frames, limit)

    compare_headers(result, golden.header, candidate.header, limit)

    for index in sorted(set(golden.frames) | set(candidate.frames)):
        left = golden.frames.get(index)
        right = candidate.frames.get(index)
        if left is None or right is None:
            add_mismatch(
                result,
                index,
                "frames",
                "present" if left else "missing",
                "present" if right else "missing",
                limit,
            )
            continue

        for channel in CHANNELS:
            if channel == "header":
                continue
            golden_value = channel_value(left, channel)
            candidate_value = channel_value(right, channel)
            if channel in NUMERIC_CHANNELS:
                matches, error = compare_numbers(golden_value, candidate_value, tolerances[channel])
                result.max_error[channel] = max(result.max_error[channel], error)
            else:
                matches = golden_value == candidate_value
            if not matches:
                add_mismatch(result, index, channel, golden_value, candidate_value, limit)

    result.status = "PASS" if result.mismatch_count == 0 else "FAIL"
    return result


# --- reporting ---------------------------------------------------------------


def format_number(value: float) -> str:
    if math.isinf(value):
        return "inf"
    if value == 0.0:
        return "0"
    return f"{value:.2e}"


def format_value(value: Any) -> str:
    if isinstance(value, str):
        return json.dumps(value)
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def max_error_summary(result: SceneResult) -> str:
    parts = [
        f"{channel} {format_number(result.max_error[channel])}"
        for channel in CHANNELS
        if channel in NUMERIC_CHANNELS
    ]
    return "max |delta|: " + ", ".join(parts)


REPORT_STREAM: Any = None


def report(*values: Any, **kwargs: Any) -> None:
    """Human-readable report line; goes to stderr when --json - owns stdout."""
    print(*values, file=REPORT_STREAM, **kwargs)


def report_scene(result: SceneResult, verbose: bool = False) -> None:
    report(f"{result.status:4s}  {result.name}")
    if result.status == "SKIP":
        report(f"        {result.reason}")
        return

    golden = result.golden_header
    candidate = result.candidate_header
    if golden is not None and candidate is not None:
        report(
            f"        golden    {result.golden_frames:>5d} frames  camera={golden.camera_class}"
            f"  pcam={golden.pcam_class}  observer={golden.observer}  godot={golden.godot}"
        )
        report(
            f"        candidate {result.candidate_frames:>5d} frames  camera={candidate.camera_class}"
            f"  pcam={candidate.pcam_class}  observer={candidate.observer}  godot={candidate.godot}"
        )

    if result.status == "PASS":
        if verbose:
            report(f"        {max_error_summary(result)}")
        return

    failing = sum(1 for channel, count in result.channel_mismatches.items() if count)
    report(f"        {result.mismatch_count} mismatch(es) in {failing} channel(s)")
    for mismatch in result.mismatches:
        report(
            f"        frame {mismatch.frame:<6d} {mismatch.channel:<22s}"
            f" golden={format_value(mismatch.golden)} candidate={format_value(mismatch.candidate)}"
        )
    report(f"        {max_error_summary(result)}")


def print_error_table(results: list[SceneResult], tolerances: dict[str, float]) -> None:
    report()
    header = f"{'channel':<10s} {'tolerance':>10s} {'max |delta|':>12s} {'mismatches':>11s}"
    report(header)
    report("-" * len(header))
    for channel in CHANNELS + ("frames",):
        count = sum(result.channel_mismatches.get(channel, 0) for result in results)
        if channel in NUMERIC_CHANNELS:
            worst = max((result.max_error[channel] for result in results), default=0.0)
            report(
                f"{channel:<10s} {format_number(tolerances[channel]):>10s}"
                f" {format_number(worst):>12s} {count:>11d}"
            )
        else:
            report(f"{channel:<10s} {'exact':>10s} {'-':>12s} {count:>11d}")


def print_summary(results: list[SceneResult]) -> str:
    passed = sum(1 for result in results if result.status == "PASS")
    failed = sum(1 for result in results if result.status == "FAIL")
    skipped = sum(1 for result in results if result.status == "SKIP")
    verdict = "FAIL" if failed else ("PASS" if passed else "SKIP")
    report()
    report(f"scenes: {passed} pass, {failed} fail, {skipped} skip - verdict: {verdict}")
    return verdict


# --- command line ------------------------------------------------------------


def parse_tolerance_overrides(values: list[str]) -> dict[str, float]:
    tolerances = dict(DEFAULT_TOLERANCES)
    for raw in values:
        channel, separator, value = raw.partition("=")
        channel = channel.strip()
        if not separator or not channel:
            raise UsageError(f"--tolerance {raw!r}: expected <channel>=<value>")
        if channel in NUMERIC_CHANNELS:
            pass
        elif channel in DEFAULT_TOLERANCES:
            raise UsageError(
                f"--tolerance {raw!r}: channel {channel!r} is compared exactly "
                f"and has no tolerance (numeric: {', '.join(sorted(NUMERIC_CHANNELS))})"
            )
        else:
            raise UsageError(
                f"--tolerance {raw!r}: unknown channel {channel!r}"
                f" (known: {', '.join(sorted(NUMERIC_CHANNELS))})"
            )
        try:
            tolerance = float(value)
        except ValueError as error:
            raise UsageError(f"--tolerance {raw!r}: {value!r} is not a number") from error
        if tolerance < 0.0:
            raise UsageError(f"--tolerance {raw!r}: tolerance must be >= 0")
        tolerances[channel] = tolerance
    return tolerances


def build_parser() -> argparse.ArgumentParser:
    repo_root = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(
        prog="pcam_compare.py",
        description="Diff Phantom Camera parity recordings (pcam-record/v1 JSONL) channel by channel.",
        epilog=(
            "channels: "
            + ", ".join(CHANNELS)
            + ", frames - exit 0 pass, 1 mismatch, 2 usage/IO (parity/AGENTS.md)"
        ),
    )
    parser.add_argument(
        "--golden",
        type=Path,
        default=repo_root / "parity" / "pcam_golden",
        help="directory of oracle recordings (default: parity/pcam_golden)",
    )
    parser.add_argument(
        "--candidate",
        type=Path,
        default=repo_root / "client" / "rust" / "target" / "pcam",
        help="directory of port recordings (default: client/rust/target/pcam)",
    )
    parser.add_argument("--scene", default="", help="only scenes whose name contains this substring")
    parser.add_argument(
        "--max-mismatches",
        type=int,
        default=5,
        metavar="N",
        help="mismatches reported per scene (default: 5)",
    )
    parser.add_argument(
        "--tolerance",
        action="append",
        default=[],
        metavar="CHANNEL=VALUE",
        help=(
            "override one numeric channel tolerance, repeatable "
            f"(channels: {', '.join(sorted(NUMERIC_CHANNELS))})"
        ),
    )
    parser.add_argument(
        "--json",
        nargs="?",
        const="-",
        default=None,
        metavar="PATH",
        help="write a machine-readable report (PATH, or - for stdout)",
    )
    parser.add_argument(
        "-v",
        "--verbose",
        action="store_true",
        help="also print the per-channel max |delta| line for passing scenes",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    global REPORT_STREAM

    parser = build_parser()
    args = parser.parse_args(argv)
    # `--json -` owns stdout, so the human report moves to stderr.
    REPORT_STREAM = sys.stderr if args.json == "-" else sys.stdout

    try:
        tolerances = parse_tolerance_overrides(args.tolerance)
        if args.max_mismatches < 1:
            raise UsageError("--max-mismatches must be >= 1")

        golden_dir: Path = args.golden
        candidate_dir: Path = args.candidate
        if not golden_dir.is_dir() and not candidate_dir.is_dir():
            raise UsageError(
                f"neither {golden_dir} nor {candidate_dir} exists: record something first "
                "(bash tools/pcam_parity.sh)"
            )

        golden_scenes = discover_scenes(golden_dir)
        candidate_scenes = discover_scenes(candidate_dir)
        names = sorted(set(golden_scenes) | set(candidate_scenes))
        if args.scene:
            names = [name for name in names if args.scene in name]
        if not names:
            raise UsageError(
                f"no .jsonl recordings matched (--scene {args.scene!r})"
                if args.scene
                else "no .jsonl recordings found"
            )

        results = [
            compare_scene(
                name,
                golden_scenes.get(name),
                candidate_scenes.get(name),
                tolerances,
                args.max_mismatches,
            )
            for name in names
        ]

        for result in results:
            report_scene(result, args.verbose)

        compared = [result for result in results if result.status in {"PASS", "FAIL"}]
        if compared:
            print_error_table(compared, tolerances)
        verdict = print_summary(results)

        if args.json is not None:
            payload = {
                "schema": "pcam-compare/v1",
                "golden_dir": str(golden_dir),
                "candidate_dir": str(candidate_dir),
                "tolerances": {key: value for key, value in tolerances.items()},
                "verdict": verdict,
                "scenes": [result.as_dict() for result in results],
            }
            rendered = json.dumps(payload, indent=2, sort_keys=True) + "\n"
            if args.json == "-":
                sys.stdout.write(rendered)
            else:
                Path(args.json).write_text(rendered, encoding="utf-8")

        if not compared:
            print("nothing compared: every scene is missing on one side", file=sys.stderr)
            return EXIT_USAGE
        return EXIT_PASS if all(result.status == "PASS" for result in compared) else EXIT_MISMATCH
    except UsageError as error:
        print(f"pcam_compare.py: error: {error}", file=sys.stderr)
        return EXIT_USAGE
    except OSError as error:
        print(f"pcam_compare.py: error: {error}", file=sys.stderr)
        return EXIT_USAGE


if __name__ == "__main__":
    sys.exit(main())
