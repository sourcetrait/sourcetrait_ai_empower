"""config.py - rust_recon calibration loader.

Reads calibration.toml (sibling file) on first import. Exposes typed
accessor helpers used by characterize.py + emit.py:

- int_param(env_name, *toml_path, default) - int with env override
- float_param(env_name, *toml_path, default) - float with env override
- frozenset_param(*toml_path, default) - frozenset (no env override)
- tuple_param(*toml_path, default) - tuple (no env override)

Env vars (ORIENT_*) take precedence over the TOML defaults. If neither
TOML nor env provides a value, the supplied default is returned.

Why: 0.0.30 moves calibration constants out of the python source so
the_user can tune the picker by editing one TOML file rather than
chasing constants across characterize.py + emit.py. Env-var override
preserved so ad-hoc tuning continues to work without TOML edits.
"""

from __future__ import annotations
import os
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover (Python < 3.11)
    tomllib = None


def _load() -> dict:
    if tomllib is None:
        return {}
    p = Path(__file__).parent / "calibration.toml"
    if not p.is_file():
        return {}
    with open(p, "rb") as f:
        return tomllib.load(f)


CONFIG = _load()


def _get_toml(*path):
    node = CONFIG
    for k in path:
        if not isinstance(node, dict):
            return None
        node = node.get(k)
        if node is None:
            return None
    return node


def int_param(env_name: str, *toml_path: str, default: int) -> int:
    """int with env override. env_name='' to skip env lookup."""
    if env_name:
        env_val = os.environ.get(env_name)
        if env_val is not None:
            return int(env_val)
    val = _get_toml(*toml_path)
    if val is None:
        return default
    return int(val)


def float_param(env_name: str, *toml_path: str, default: float) -> float:
    """float with env override. env_name='' to skip env lookup."""
    if env_name:
        env_val = os.environ.get(env_name)
        if env_val is not None:
            return float(env_val)
    val = _get_toml(*toml_path)
    if val is None:
        return default
    return float(val)


def frozenset_param(*toml_path: str, default: frozenset = frozenset()) -> frozenset:
    """frozenset from a TOML list. No env override (sets aren't naturally
    expressible as env vars; use TOML for these)."""
    val = _get_toml(*toml_path)
    if val is None:
        return default
    return frozenset(val)


def tuple_param(*toml_path: str, default: tuple = ()) -> tuple:
    """tuple from a TOML list. No env override (same reasoning as
    frozenset_param)."""
    val = _get_toml(*toml_path)
    if val is None:
        return default
    return tuple(val)
