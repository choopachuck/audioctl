"""Rescan, and make one endpoint the default."""

from __future__ import annotations

from homeassistant.components.button import ButtonEntity
from homeassistant.config_entries import ConfigEntry
from homeassistant.const import Platform
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from .codec import device_set_topic, hub_topic
from .const import DOMAIN
from .entity import AudioDeviceEntity, AudioNodeEntity
from .hub import AudioHub


async def async_setup_entry(hass: HomeAssistant, entry: ConfigEntry, async_add_entities: AddEntitiesCallback) -> None:
    hub: AudioHub = hass.data[DOMAIN][entry.entry_id]
    hub.register_node_platform(Platform.BUTTON, async_add_entities, AudioRescan)
    hub.register_device_platform(Platform.BUTTON, async_add_entities, AudioMakeDefault)


class AudioRescan(AudioNodeEntity, ButtonEntity):
    """Ask audioctl to look for endpoints again."""

    def __init__(self, hub: AudioHub, node_id: str, _kind: str) -> None:
        super().__init__(hub, node_id, "rescan")

    async def async_press(self) -> None:
        await self.hub.async_publish(hub_topic(self.hub.base_topic, self.node_id, "rescan"), "PRESS")


class AudioMakeDefault(AudioDeviceEntity, ButtonEntity):
    """Make this endpoint the default for its direction."""

    def __init__(self, hub: AudioHub, node_id: str, key: str, _kind: str) -> None:
        super().__init__(hub, node_id, key, "default")

    async def async_press(self) -> None:
        await self.hub.async_publish(device_set_topic(self.hub.base_topic, self.node_id, self.key, "default"), "PRESS")
