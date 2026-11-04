"""A command line sent to audioctl."""

from __future__ import annotations

from homeassistant.components.text import TextEntity
from homeassistant.config_entries import ConfigEntry
from homeassistant.const import Platform
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from .codec import hub_topic
from .const import DOMAIN
from .entity import AudioNodeEntity
from .hub import AudioHub


async def async_setup_entry(hass: HomeAssistant, entry: ConfigEntry, async_add_entities: AddEntitiesCallback) -> None:
    hub: AudioHub = hass.data[DOMAIN][entry.entry_id]
    hub.register_node_platform(Platform.TEXT, async_add_entities, AudioCommand)


class AudioCommand(AudioNodeEntity, TextEntity):
    """The same command language as the CLI and the tray hotkeys."""

    _attr_native_value = ""

    def __init__(self, hub: AudioHub, node_id: str, _kind: str) -> None:
        super().__init__(hub, node_id, "command")

    async def async_set_value(self, value: str) -> None:
        self._attr_native_value = value
        self.schedule_state()
        await self.hub.async_publish(hub_topic(self.hub.base_topic, self.node_id, "command"), value)
