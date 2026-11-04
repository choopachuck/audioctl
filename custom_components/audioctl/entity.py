"""Shared entity base classes."""

from __future__ import annotations

from typing import TYPE_CHECKING

from homeassistant.helpers.device_registry import DeviceInfo
from homeassistant.helpers.entity import Entity

from .codec import DeviceInfo as AudioDeviceInfo
from .codec import device_identifier, entity_unique_id, hub_identifier
from .const import DOMAIN, ENTITY_NAMES, ICONS

if TYPE_CHECKING:
    from .hub import AudioHub


class AudioBaseEntity(Entity):
    """An entity whose availability follows the audioctl process."""

    _attr_has_entity_name = True
    _attr_should_poll = False

    def __init__(self, hub: AudioHub, node_id: str) -> None:
        self.hub = hub
        self.node_id = node_id
        self._removed = False

    @property
    def available(self) -> bool:
        return not self._removed and self.hub.is_online(self.node_id)

    async def async_added_to_hass(self) -> None:
        await super().async_added_to_hass()
        if self._removed:
            await self.async_remove()

    async def mark_removed(self) -> None:
        self._removed = True
        if self.hass is not None:
            await self.async_remove()

    def schedule_state(self) -> None:
        if self.hass is not None and not self._removed:
            self.async_write_ha_state()


class AudioNodeEntity(AudioBaseEntity):
    """A control attached to the computer, not to one endpoint."""

    def __init__(self, hub: AudioHub, node_id: str, kind: str) -> None:
        super().__init__(hub, node_id)
        self.kind = kind
        self._attr_unique_id = entity_unique_id(node_id, kind)
        self._attr_translation_key = kind
        self._attr_name = ENTITY_NAMES.get(kind, kind)
        self._attr_icon = ICONS.get(kind)

    @property
    def device_info(self) -> DeviceInfo:
        node = self.hub.node_state(self.node_id)
        return DeviceInfo(
            identifiers={(DOMAIN, hub_identifier(self.node_id))},
            name=self.hub.node_name(self.node_id),
            manufacturer="audioctl",
            model="audioctl",
            sw_version=None if node is None else node.sw_version,
        )


class AudioDeviceEntity(AudioBaseEntity):
    """A control attached to one playback or capture endpoint."""

    def __init__(self, hub: AudioHub, node_id: str, key: str, kind: str) -> None:
        super().__init__(hub, node_id)
        self.key = key
        self.kind = kind
        self._attr_unique_id = entity_unique_id(node_id, kind, key)
        self._attr_translation_key = kind
        self._attr_name = ENTITY_NAMES.get(kind, kind)
        self._attr_icon = ICONS.get(kind)

    def _info(self) -> AudioDeviceInfo | None:
        return self.hub.device_info(self.node_id, self.key)

    @property
    def available(self) -> bool:
        return super().available and self._info() is not None

    @property
    def device_info(self) -> DeviceInfo:
        info = self._info()
        return DeviceInfo(
            identifiers={(DOMAIN, device_identifier(self.node_id, self.key))},
            name=info.name if info is not None else self.key,
            manufacturer=None if info is None else info.adapter,
            model=None if info is None else info.form_factor,
            via_device=(DOMAIN, hub_identifier(self.node_id)),
        )
