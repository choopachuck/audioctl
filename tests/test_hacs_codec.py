"""Parser tests. They load codec.py directly so Home Assistant is not required."""

from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / "custom_components" / "audioctl"
spec = importlib.util.spec_from_file_location("audioctl_codec", ROOT / "codec.py")
assert spec is not None and spec.loader is not None
codec = importlib.util.module_from_spec(spec)
sys.modules["audioctl_codec"] = codec
spec.loader.exec_module(codec)


DEVICE = """
{
  "key": "ignored",
  "id": "{0.0.0.00000000}.{spk}",
  "name": "Headphones",
  "flow": "render",
  "form_factor": "headphones",
  "adapter": "Realtek",
  "volume": 40,
  "muted": false,
  "default": true
}
"""


def test_parse_topic_classifies_retained_messages():
    assert codec.parse_topic("audioctl/", "audioctl/desktop/status").kind is codec.TopicKind.STATUS
    hub = codec.parse_topic("audioctl", "audioctl/desktop/info")
    assert hub.kind is codec.TopicKind.HUB_INFO
    assert hub.node_id == "desktop"
    assert hub.device_key is None
    device = codec.parse_topic("audioctl", "audioctl/desktop/speakers/info")
    assert device.kind is codec.TopicKind.DEVICE_INFO
    assert device.device_key == "speakers"


def test_parse_topic_ignores_state_commands_and_other_prefixes():
    assert codec.parse_topic("audioctl", "audioctl/desktop/speakers/state") is None
    assert codec.parse_topic("audioctl", "audioctl/desktop/speakers/volume/set") is None
    assert codec.parse_topic("audioctl", "audioctl/desktop/command") is None
    assert codec.parse_topic("audioctl", "audioctl2/desktop/status") is None
    assert codec.parse_topic("audioctl", "audioctl/desktop//info") is None


def test_node_filter():
    assert codec.parse_topic("audioctl", "audioctl/laptop/status", "desktop") is None
    kept = codec.parse_topic("audioctl", "audioctl/desktop/status", " desktop ")
    assert kept is not None
    assert kept.node_id == "desktop"


def test_subscription_topics():
    assert codec.subscription_topics("audioctl/") == (
        "audioctl/+/status",
        "audioctl/+/info",
        "audioctl/+/+/info",
    )


def test_decode_device_uses_the_topic_key():
    info = codec.decode_device_payload(DEVICE, "speakers")
    assert info is not None
    assert info.key == "speakers"
    assert info.name == "Headphones"
    assert info.volume == 40
    assert info.default is True
    assert info.flow == "render"
    assert codec.entity_kinds(info) == {"volume", "mute", "default"}
    assert codec.platform_for("volume") == "number"
    assert codec.platform_for("mute") == "switch"
    assert codec.platform_for("output") == "select"
    assert codec.entity_unique_id("desktop", "volume", "speakers") == "audioctl_desktop_speakers_volume"


def test_empty_payload_removes_the_device():
    assert codec.decode_device_payload("  ", "speakers") is None


def test_hub_info_lists_devices():
    payload = json.dumps({"name": "audioctl (pc)", "output": "Headphones", "outputs": ["Headphones", "Speakers"], "input": "Mic", "inputs": ["Mic"]})
    info = codec.parse_hub_info(payload, "pc")
    assert info.output == "Headphones"
    assert info.outputs == ["Headphones", "Speakers"]
    assert info.input == "Mic"


def test_checked_topics_reject_wildcards():
    assert codec.checked_base_topic(" audioctl ") == "audioctl"
    try:
        codec.checked_base_topic("audio/+/ctl")
    except ValueError as err:
        assert str(err) == "invalid_base_topic"
    else:
        raise AssertionError("wildcard base topic was accepted")
    assert codec.checked_node_filter("  ") is None
