"""MQTT topic and payload parsing for audioctl.

This module does not import Home Assistant. The integration platforms only
display the model built here.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from enum import Enum
from typing import Any

DEFAULT_BASE_TOPIC = "audioctl"
DEVICE_KINDS = ("volume", "mute", "default")
NODE_KINDS = ("output", "input", "command", "rescan")


class TopicKind(Enum):
    """Which retained audioctl topic a message belongs to."""

    STATUS = "status"
    HUB_INFO = "hub_info"
    DEVICE_INFO = "device_info"


@dataclass(frozen=True)
class ParsedTopic:
    """An audioctl topic addressed to one computer, and maybe one device."""

    kind: TopicKind
    node_id: str
    device_key: str | None = None


@dataclass
class HubInfo:
    """Retained description of one audioctl process."""

    node_id: str
    name: str
    sw_version: str | None
    output: str | None
    input: str | None
    outputs: list[str]
    inputs: list[str]


@dataclass
class DeviceInfo:
    """Retained description of one endpoint. The key comes from the topic."""

    key: str
    id: str | None
    name: str
    flow: str
    form_factor: str | None
    adapter: str | None
    volume: int
    muted: bool
    default: bool


def normalize_base_topic(topic: str | None) -> str:
    return (topic or "").strip().strip("/")


def normalize_node_filter(node: str | None) -> str | None:
    if node is None:
        return None
    text = node.strip()
    return text or None


def checked_base_topic(topic: str) -> str:
    """Return a base topic or raise ValueError('invalid_base_topic')."""
    base = normalize_base_topic(topic)
    if not base or any(char in base for char in "+# \t"):
        raise ValueError("invalid_base_topic")
    return base


def checked_node_filter(node: str) -> str | None:
    """Return a node id, None when empty, or raise ValueError('invalid_node_id')."""
    text = normalize_node_filter(node)
    if text is None:
        return None
    if any(char in text for char in "+#/ \t"):
        raise ValueError("invalid_node_id")
    return text


def merge_settings(
    data: dict[str, Any],
    options: dict[str, Any] | None = None,
    *,
    base_key: str = "base_topic",
    node_key: str = "node_id",
    default_base: str = DEFAULT_BASE_TOPIC,
) -> tuple[str, str | None]:
    """Options override the original config entry data."""
    merged = dict(data)
    if options:
        merged.update(options)
    raw_base = merged.get(base_key)
    base = normalize_base_topic(raw_base if isinstance(raw_base, str) else default_base)
    raw_node = merged.get(node_key)
    return base or default_base, normalize_node_filter(raw_node if isinstance(raw_node, str) else None)


def subscription_topics(base_topic: str) -> tuple[str, str, str]:
    """Status, hub info, and per-device info subscriptions."""
    root = normalize_base_topic(base_topic)
    return (f"{root}/+/status", f"{root}/+/info", f"{root}/+/+/info")


def parse_topic(base_topic: str, topic: str, node_filter: str | None = None) -> ParsedTopic | None:
    """Classify a topic. State and command topics are ignored on purpose."""
    root = normalize_base_topic(base_topic)
    if not root:
        return None
    prefix = f"{root}/"
    if not topic.startswith(prefix):
        return None
    parts = topic[len(prefix) :].split("/")
    device_key: str | None = None
    if len(parts) == 2 and parts[0] and parts[1] == "status":
        kind = TopicKind.STATUS
        node_id = parts[0]
    elif len(parts) == 2 and parts[0] and parts[1] == "info":
        kind = TopicKind.HUB_INFO
        node_id = parts[0]
    elif len(parts) == 3 and parts[0] and parts[1] and parts[2] == "info":
        kind = TopicKind.DEVICE_INFO
        node_id = parts[0]
        device_key = parts[1]
    else:
        return None
    wanted = normalize_node_filter(node_filter)
    if wanted is not None and node_id != wanted:
        return None
    return ParsedTopic(kind=kind, node_id=node_id, device_key=device_key)


def hub_topic(base_topic: str, node_id: str, leaf: str) -> str:
    return f"{normalize_base_topic(base_topic)}/{node_id}/{leaf}"


def device_set_topic(base_topic: str, node_id: str, key: str, field: str) -> str:
    return f"{normalize_base_topic(base_topic)}/{node_id}/{key}/{field}/set"


def hub_identifier(node_id: str) -> str:
    return f"audioctl_{node_id}"


def device_identifier(node_id: str, key: str) -> str:
    return f"audioctl_{node_id}_{key}"


def entity_unique_id(node_id: str, kind: str, key: str | None = None) -> str:
    if key:
        return f"audioctl_{node_id}_{key}_{kind}"
    return f"audioctl_{node_id}_{kind}"


def platform_for(kind: str) -> str:
    """Home Assistant platform domain that owns this entity kind."""
    if kind == "volume":
        return "number"
    if kind == "mute":
        return "switch"
    if kind in ("default", "rescan"):
        return "button"
    if kind in ("output", "input"):
        return "select"
    if kind == "command":
        return "text"
    raise KeyError(kind)


def entity_kinds(info: DeviceInfo) -> set[str]:
    return set(DEVICE_KINDS)


def _string_list(value: Any) -> list[str]:
    if not isinstance(value, list):
        return []
    return [item for item in value if isinstance(item, str)]


def parse_hub_info(payload: str, node_id: str) -> HubInfo:
    raw = json.loads(payload) if payload.strip() else {}
    if not isinstance(raw, dict):
        raw = {}
    name = raw.get("name")
    return HubInfo(
        node_id=node_id,
        name=name if isinstance(name, str) and name else f"audioctl ({node_id})",
        sw_version=raw.get("sw_version") if isinstance(raw.get("sw_version"), str) else None,
        output=raw.get("output") if isinstance(raw.get("output"), str) else None,
        input=raw.get("input") if isinstance(raw.get("input"), str) else None,
        outputs=_string_list(raw.get("outputs")),
        inputs=_string_list(raw.get("inputs")),
    )


def decode_device_payload(payload: str, key: str) -> DeviceInfo | None:
    """An empty payload removes the device. Volume is a plain integer, 0–100."""
    if not payload.strip():
        return None
    raw = json.loads(payload)
    if not isinstance(raw, dict):
        return None
    volume = raw.get("volume", 0)
    if isinstance(volume, bool) or not isinstance(volume, (int, float)):
        volume = 0
    volume = max(0, min(100, int(volume)))
    name = raw.get("name")
    return DeviceInfo(
        key=key,
        id=raw.get("id") if isinstance(raw.get("id"), str) else None,
        name=name if isinstance(name, str) and name else key,
        flow=raw.get("flow") if raw.get("flow") in ("render", "capture") else "render",
        form_factor=raw.get("form_factor") if isinstance(raw.get("form_factor"), str) else None,
        adapter=raw.get("adapter") if isinstance(raw.get("adapter"), str) else None,
        volume=volume,
        muted=bool(raw.get("muted")),
        default=bool(raw.get("default")),
    )
