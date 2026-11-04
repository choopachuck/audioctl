"""The audioctl Home Assistant integration."""

from __future__ import annotations

import logging

from homeassistant.components import mqtt
from homeassistant.config_entries import ConfigEntry
from homeassistant.const import Platform
from homeassistant.core import HomeAssistant

from .const import DOMAIN
from .hub import AudioHub

_LOGGER = logging.getLogger(__name__)

PLATFORMS = [
    Platform.NUMBER,
    Platform.SWITCH,
    Platform.SELECT,
    Platform.BUTTON,
    Platform.TEXT,
]


async def async_setup_entry(hass: HomeAssistant, entry: ConfigEntry) -> bool:
    """Subscribe to audioctl and forward every platform."""
    if not await mqtt.async_wait_for_mqtt_client(hass):
        _LOGGER.error("MQTT client is not available")
        return False
    hub = AudioHub(hass, entry)
    hass.data.setdefault(DOMAIN, {})[entry.entry_id] = hub
    try:
        await hass.config_entries.async_forward_entry_setups(entry, PLATFORMS)
        await hub.async_start()
    except Exception:
        _LOGGER.exception("audioctl setup failed")
        hub.unsubscribe()
        await hass.config_entries.async_unload_platforms(entry, PLATFORMS)
        hass.data[DOMAIN].pop(entry.entry_id, None)
        return False
    entry.async_on_unload(entry.add_update_listener(async_reload_entry))
    return True


async def async_unload_entry(hass: HomeAssistant, entry: ConfigEntry) -> bool:
    """Stop MQTT subscriptions and unload platforms."""
    hub: AudioHub | None = hass.data.get(DOMAIN, {}).get(entry.entry_id)
    if hub is not None:
        hub.unsubscribe()
    unloaded = await hass.config_entries.async_unload_platforms(entry, PLATFORMS)
    if unloaded:
        hass.data[DOMAIN].pop(entry.entry_id, None)
    return unloaded


async def async_reload_entry(hass: HomeAssistant, entry: ConfigEntry) -> None:
    """Reload when the base topic or node filter changes."""
    await hass.config_entries.async_reload(entry.entry_id)
