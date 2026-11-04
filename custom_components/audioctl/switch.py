"""Mute switch for one endpoint."""

from __future__ import annotations

from homeassistant.components.switch import SwitchEntity
from homeassistant.config_entries import ConfigEntry
from homeassistant.const import Platform
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from .codec import device_set_topic
from .const import DOMAIN
from .entity import AudioDeviceEntity
from .hub import AudioHub


async def async_setup_entry(hass: HomeAssistant, entry: ConfigEntry, async_add_entities: AddEntitiesCallback) -> None:
    hub: AudioHub = hass.data[DOMAIN][entry.entry_id]
    hub.register_device_platform(Platform.SWITCH, async_add_entities, AudioMute)


class AudioMute(AudioDeviceEntity, SwitchEntity):
    """On means the endpoint is muted."""

    @property
    def is_on(self) -> bool:
        info = self._info()
        return False if info is None else info.muted

    async def async_turn_on(self, **_kwargs) -> None:
        await self._set(True)

    async def async_turn_off(self, **_kwargs) -> None:
        await self._set(False)

    async def _set(self, muted: bool) -> None:
        info = self._info()
        if info is not None:
            info.muted = muted
            self.schedule_state()
        await self.hub.async_publish(
            device_set_topic(self.hub.base_topic, self.node_id, self.key, "mute"),
            "ON" if muted else "OFF",
        )
