"""Config flow for audioctl."""

from __future__ import annotations

from typing import Any

import voluptuous as vol
from homeassistant.components.mqtt.const import DOMAIN as MQTT_DOMAIN
from homeassistant.config_entries import ConfigEntry, ConfigFlow, OptionsFlow
from homeassistant.core import callback
from homeassistant.data_entry_flow import FlowResult

from .codec import checked_base_topic, checked_node_filter, merge_settings
from .const import CONF_BASE_TOPIC, CONF_NODE_ID, DEFAULT_BASE_TOPIC, DOMAIN


def _schema(base: str, node: str) -> vol.Schema:
    return vol.Schema(
        {
            vol.Required(CONF_BASE_TOPIC, default=base): str,
            vol.Optional(CONF_NODE_ID, default=node): str,
        }
    )


def _validate(user_input: dict[str, Any]) -> tuple[dict[str, str], str, str | None]:
    errors: dict[str, str] = {}
    base = DEFAULT_BASE_TOPIC
    node: str | None = None
    try:
        base = checked_base_topic(str(user_input.get(CONF_BASE_TOPIC, "")))
    except ValueError as err:
        errors[CONF_BASE_TOPIC] = str(err)
    try:
        node = checked_node_filter(str(user_input.get(CONF_NODE_ID, "")))
    except ValueError as err:
        errors[CONF_NODE_ID] = str(err)
    return errors, base, node


class AudioConfigFlow(ConfigFlow, domain=DOMAIN):
    """One entry listens to every audioctl computer, or to a single node id."""

    VERSION = 1

    @staticmethod
    @callback
    def async_get_options_flow(_config_entry: ConfigEntry) -> AudioOptionsFlow:
        return AudioOptionsFlow()

    async def async_step_user(self, user_input: dict[str, Any] | None = None) -> FlowResult:
        if self._async_current_entries():
            return self.async_abort(reason="already_configured")
        if not self.hass.config_entries.async_entries(MQTT_DOMAIN):
            return self.async_abort(reason="mqtt_missing")
        errors: dict[str, str] = {}
        base = DEFAULT_BASE_TOPIC
        node_text = ""
        if user_input is not None:
            errors, base, node = _validate(user_input)
            if errors:
                base = str(user_input.get(CONF_BASE_TOPIC, base))
                node_text = str(user_input.get(CONF_NODE_ID, ""))
            else:
                node_text = node or ""
            if not errors:
                title = f"audioctl ({node})" if node else "audioctl"
                return self.async_create_entry(title=title, data={CONF_BASE_TOPIC: base, CONF_NODE_ID: node or ""})
        return self.async_show_form(step_id="user", data_schema=_schema(base, node_text), errors=errors)


class AudioOptionsFlow(OptionsFlow):
    """Change the base topic or limit the entry to one computer."""

    async def async_step_init(self, user_input: dict[str, Any] | None = None) -> FlowResult:
        current_base, current_node = merge_settings(
            dict(self.config_entry.data),
            dict(self.config_entry.options),
            base_key=CONF_BASE_TOPIC,
            node_key=CONF_NODE_ID,
            default_base=DEFAULT_BASE_TOPIC,
        )
        if user_input is not None:
            errors, base, node = _validate(user_input)
            if not errors:
                return self.async_create_entry(data={CONF_BASE_TOPIC: base, CONF_NODE_ID: node or ""})
            current_base = str(user_input.get(CONF_BASE_TOPIC, current_base))
            current_node = str(user_input.get(CONF_NODE_ID, ""))
            return self.async_show_form(step_id="init", data_schema=_schema(current_base, current_node or ""), errors=errors)
        return self.async_show_form(step_id="init", data_schema=_schema(current_base, current_node or ""))
