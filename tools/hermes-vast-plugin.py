#!/usr/bin/env python3
"""VAST Platform Adapter for Hermes Agent Gateway.

Drop this file into the Hermes Agent repository at:
    gateway/platforms/vast.py

Then Hermes Agent will automatically detect it as a platform plugin.
Configure it in your Hermes Agent config.yaml:

    platforms:
      - type: vast
        connection_key: "your-connection-key-from-vast-admin"
        ws_url: "ws://your-vast-server:3000/ws/bot"

Requirements (usually already installed by Hermes Agent):
    pip install websockets
"""

import asyncio
import json
import logging
from dataclasses import dataclass, field
from typing import Any, Callable, Optional

import websockets

# ── Hermes Gateway imports (adjust paths if your Hermes layout differs) ─────
from gateway.platform_registry import PlatformEntry, platform_registry
from gateway.config import PlatformConfig

log = logging.getLogger("hermes.platform.vast")


# ═══════════════════════════════════════════════════════════════════════════
# Platform configuration
# ═══════════════════════════════════════════════════════════════════════════

@dataclass
class VASTPlatformConfig:
    connection_key: str = ""
    ws_url: str = "ws://localhost:3000/ws/bot"


# ═══════════════════════════════════════════════════════════════════════════
# Adapter implementation
# ═══════════════════════════════════════════════════════════════════════════

class VASTAdapter:
    """Bridges Hermes Agent's gateway stream to a VAST IM server.

    Opens a persistent WebSocket to VAST, receives BotMention events
    (which Hermes processes as incoming messages), and sends replies
    back via BotReply events.
    """

    # ── Class-level flags (as expected by GatewayStreamConsumer) ────────
    REQUIRES_EDIT_FINALIZE: bool = False
    RESEND_FINAL_ON_EMPTY_STREAM_FALLBACK: bool = False
    MAX_MESSAGE_LENGTH: int = 4000

    def __init__(self, config: PlatformConfig):
        raw = config.platform_config or {}
        self._cfg = VASTPlatformConfig(
            connection_key=(raw.get("connection_key")
                            or raw.get("VAST_CONNECTION_KEY", "")),
            ws_url=(raw.get("ws_url")
                    or raw.get("VAST_WS_URL", "ws://localhost:3000/ws/bot")),
        )
        self._ws: Optional[websockets.WebSocketClientProtocol] = None
        self._pending_mentions: dict[str, dict] = {}  # mention_id -> metadata
        self._stream_callback: Optional[Callable] = None
        self._connected = False

    # ── Connection management ───────────────────────────────────────────

    async def connect(self) -> None:
        """Open the WebSocket to VAST and start the read loop."""
        url = f"{self._cfg.ws_url}?key={self._cfg.connection_key}"
        log.info("Connecting to VAST: %s", url[:80])

        self._ws = await websockets.connect(
            url,
            ping_interval=30,
            ping_timeout=10,
        )
        self._connected = True
        log.info("VAST connection established")

        # Start the background read loop
        asyncio.create_task(self._read_loop())

    async def disconnect(self) -> None:
        self._connected = False
        if self._ws:
            await self._ws.close()
            self._ws = None

    async def _read_loop(self) -> None:
        """Continuously read events from VAST."""
        while self._connected and self._ws:
            try:
                raw = await self._ws.recv()
                event = json.loads(raw)
                await self._handle_event(event)
            except websockets.ConnectionClosed:
                log.warning("VAST connection closed")
                self._connected = False
                break
            except Exception:
                log.exception("Error in VAST read loop")
                break

    async def _handle_event(self, event: dict) -> None:
        """Dispatch incoming VAST events."""
        etype = event.get("type", "")

        if etype == "bot_mention":
            # Convert VAST messages to Hermes format and feed to gateway
            mention_id = event["mention_id"]
            self._pending_mentions[mention_id] = {
                "channel_id": event["channel_id"],
            }
            # The Gateway will call self.send() when it produces a reply.
            # We stash the mention_id so we know where to route the reply.
            if self._stream_callback:
                messages = event.get("messages", [])
                await self._stream_callback(
                    chat_id=mention_id,
                    text=self._build_prompt(messages),
                    reply_to=None,
                )

        elif etype == "pong":
            log.debug("Heartbeat pong")

    def _build_prompt(self, messages: list[dict]) -> str:
        """Build a prompt string from the message context."""
        lines = []
        for msg in messages:
            role = msg.get("role", "user")
            content = msg.get("content", "")
            if role == "system":
                lines.append(f"[System] {content}")
            elif role == "user":
                lines.append(content)
        return "\n".join(lines)

    # ── Gateway adapter interface ───────────────────────────────────────

    async def send(
        self,
        chat_id: str,
        content: str,
        reply_to: Optional[str] = None,
        metadata: Optional[dict] = None,
    ) -> Any:
        """Called by Hermes Gateway to deliver a message to VAST."""
        if not self._ws or not self._connected:
            return _Result(False, error="Not connected")

        mention_id = chat_id  # chat_id IS the mention_id in our model
        channel_id = self._pending_mentions.get(mention_id, {}).get(
            "channel_id", ""
        )

        reply_event = {
            "type": "bot_reply",
            "mention_id": mention_id,
            "channel_id": channel_id,
            "text": content,
        }
        try:
            await self._ws.send(json.dumps(reply_event, ensure_ascii=False))
            log.info("Reply sent: mention_id=%s, %d chars", mention_id, len(content))
            # Clean up
            self._pending_mentions.pop(mention_id, None)
            return _Result(True, message_id=mention_id)
        except Exception as exc:
            log.error("Failed to send reply: %s", exc)
            return _Result(False, error=str(exc))

    async def edit_message(
        self,
        chat_id: str,
        message_id: str,
        content: str,
        finalize: bool = False,
        metadata: Optional[dict] = None,
    ) -> Any:
        """VAST does not support message editing — downgrade to send."""
        if finalize:
            return await self.send(chat_id, content)
        return _Result(True, message_id=message_id)

    async def delete_message(self, chat_id: str, message_id: str) -> None:
        """No-op: VAST does not support message deletion via bot."""
        pass

    def supports_draft_streaming(
        self, chat_type: Optional[str] = None, metadata: Optional[dict] = None
    ) -> bool:
        return False

    def message_len_fn_for_chat(self, chat_id: str) -> Callable[[str], int]:
        return len

    def max_message_length_for_chat(self, chat_id: str) -> int:
        return self.MAX_MESSAGE_LENGTH

    def truncate_message(
        self, text: str, limit: int, len_fn: Callable[[str], int] = len
    ) -> list[str]:
        if len_fn(text) <= limit:
            return [text]
        return [text[:limit]]

    def prefers_fresh_final_streaming(
        self, text: str, metadata: Optional[dict] = None
    ) -> bool:
        return False

    @staticmethod
    def strip_media_directives_for_display(text: str) -> str:
        return text


# ═══════════════════════════════════════════════════════════════════════════
# Result stub (matches Hermes gateway's duck-typed expectation)
# ═══════════════════════════════════════════════════════════════════════════

@dataclass
class _Result:
    success: bool
    message_id: Optional[str] = None
    error: Optional[str] = None
    continuation_message_ids: list[str] = field(default_factory=list)
    raw_response: Optional[dict] = None
    retry_after: Optional[int] = None
    retryable: bool = False


# ═══════════════════════════════════════════════════════════════════════════
# Plugin registration — called automatically by Hermes Agent on startup
# ═══════════════════════════════════════════════════════════════════════════

def register() -> None:
    platform_registry.register(PlatformEntry(
        name="vast",
        label="VAST IM",
        adapter_factory=lambda cfg: VASTAdapter(cfg),
        check_fn=lambda: True,
        required_env=["VAST_CONNECTION_KEY"],
        install_hint="pip install websockets",
        emoji="💬",
        platform_hint="You are a helpful assistant in VAST, a self-hosted team chat platform.",
    ))
