"""Output and input device selectors."""

from __future__ import annotations

from homeassistant.components.select import SelectEntity
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
    hub.register_node_platform(Platform.SELECT, async_add_entities, AudioSelect)


class AudioSelect(AudioNodeEntity, SelectEntity):
    """Pick the default playback or capture device. Options come from hub info."""

    def __init__(self, hub: AudioHub, node_id: str, kind: str) -> None:
        super().__init__(hub, node_id, kind)

    @property
    def options(self) -> list[str]:
        node = self.hub.node_state(self.node_id)
        if node is None:
            return []
        return list(node.outputs if self.kind == "output" else node.inputs)

    @property
    def current_option(self) -> str | None:
        node = self.hub.node_state(self.node_id)
        if node is None:
            return None
        current = node.output if self.kind == "output" else node.input
        return current if current in self.options else None

    async def async_select_option(self, option: str) -> None:
        node = self.hub.node_state(self.node_id)
        if node is not None:
            if self.kind == "output":
                node.output = option
            else:
                node.input = option
            self.schedule_state()
        leaf = "output/set" if self.kind == "output" else "input/set"
        await self.hub.async_publish(hub_topic(self.hub.base_topic, self.node_id, leaf), option)
