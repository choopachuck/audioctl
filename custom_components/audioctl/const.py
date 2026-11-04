"""Constants for the audioctl integration."""

DOMAIN = "audioctl"

CONF_BASE_TOPIC = "base_topic"
CONF_NODE_ID = "node_id"
DEFAULT_BASE_TOPIC = "audioctl"

ENTITY_NAMES = {
    "volume": "Volume",
    "mute": "Mute",
    "default": "Make default",
    "output": "Output device",
    "input": "Input device",
    "command": "Command",
    "rescan": "Rescan",
}

ICONS = {
    "volume": "mdi:volume-high",
    "mute": "mdi:volume-mute",
    "default": "mdi:check",
    "output": "mdi:speaker",
    "input": "mdi:microphone",
    "command": "mdi:console-line",
    "rescan": "mdi:refresh",
}
