"""Volume slider for one endpoint."""

from __future__ import annotations

from homeassistant.components.number import NumberEntity, NumberMode
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
    hub.register_device_platform(Platform.NUMBER, async_add_entities, AudioVolume)


class AudioVolume(AudioDeviceEntity, NumberEntity):
    """Endpoint volume, 0–100."""

    _attr_mode = NumberMode.SLIDER
    _attr_native_min_value = 0
    _attr_native_max_value = 100
    _attr_native_step = 1

    @property
    def native_value(self) -> float | None:
        info = self._info()
        return None if info is None else info.volume

    async def async_set_native_value(self, value: float) -> None:
        info = self._info()
        if info is not None:
            info.volume = int(value)
            self.schedule_state()
        await self.hub.async_publish(
            device_set_topic(self.hub.base_topic, self.node_id, self.key, "volume"),
            str(int(value)),
        )
