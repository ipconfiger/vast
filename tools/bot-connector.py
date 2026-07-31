#!/usr/bin/env python3
"""VAST Bot Connector — connects bots to VAST via WebSocket and calls LLM APIs.

Usage:
    python3 bot-connector.py --config bot-connector.yaml

Requirements:
    pip install websockets httpx pyyaml

Architecture:
    For each bot in the config file, a persistent WebSocket connection is
    opened to VAST's /ws/bot endpoint.  When VAST sends a BotMention event
    (triggered by a user @mention), the connector forwards the channel
    history to the configured LLM API and sends the reply back over the
    same WebSocket.
"""

import asyncio
import json
import logging
import os
import signal
import sys
from pathlib import Path

import httpx
import websockets
import yaml

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
)
log = logging.getLogger("vast-connector")


async def handle_bot(cfg: dict, vast_url: str) -> None:
    """Persistent loop for a single bot connection."""
    key = cfg["connection_key"]
    llm = cfg["llm"]
    system_prompt = cfg.get("system_prompt", "")

    while True:
        try:
            url = f"{vast_url}?key={key}"
            log.info("Connecting to VAST (key=%s...)", key[:8])
            async with websockets.connect(url, ping_interval=30, ping_timeout=10) as ws:
                log.info("Connected")

                async for raw in ws:
                    try:
                        event = json.loads(raw)
                    except json.JSONDecodeError:
                        log.debug("Skipping non-JSON message")
                        continue

                    etype = event.get("type", "")
                    if etype == "bot_mention":
                        await on_mention(ws, event, llm, system_prompt)
                    elif etype == "pong":
                        log.debug("Heartbeat pong")
                    elif etype == "presence":
                        log.debug("Presence: %s -> %s", event.get("user_id"), event.get("status"))
                    else:
                        log.debug("Ignored event: %s", etype)

        except (websockets.ConnectionClosed, OSError, asyncio.TimeoutError) as exc:
            log.warning("Connection lost (%s), reconnecting in 5s...", type(exc).__name__)
        except Exception:
            log.exception("Unexpected error")
        await asyncio.sleep(5)


async def on_mention(
    ws: websockets.WebSocketClientProtocol,
    event: dict,
    llm: dict,
    system_prompt: str,
) -> None:
    """Handle a BotMention event: call LLM, send BotReply."""
    mention_id = event["mention_id"]
    channel_id = event["channel_id"]
    messages = event.get("messages", [])

    # Prepend system prompt if configured
    if system_prompt:
        messages = [{"role": "system", "content": system_prompt}] + messages

    log.info(
        "BotMention mention_id=%s channel=%s msg_count=%d",
        mention_id, channel_id, len(messages),
    )

    try:
        timeout = llm.get("timeout", 120)
        async with httpx.AsyncClient(timeout=httpx.Timeout(timeout)) as client:
            resp = await client.post(
                f"{llm['url']}/chat/completions",
                headers={"Authorization": f"Bearer {llm.get('api_key', '')}"},
                json={
                    "model": llm.get("model", "hermes"),
                    "messages": messages,
                    "stream": False,
                },
            )
            resp.raise_for_status()
            reply = resp.json()["choices"][0]["message"]["content"]
    except Exception:
        log.exception("LLM call failed for mention %s", mention_id)
        reply = "抱歉，处理请求时出错了，请稍后重试。"

    reply_event = {
        "type": "bot_reply",
        "mention_id": mention_id,
        "channel_id": channel_id,
        "text": reply,
    }
    await ws.send(json.dumps(reply_event, ensure_ascii=False))
    log.info("Reply sent for mention_id=%s (%d chars)", mention_id, len(reply))


async def main(config_path: str) -> None:
    """Load config and start one connection loop per bot."""
    with open(config_path) as f:
        raw_text = f.read()
    # Expand ${VAR} references against the environment so secrets can be
    # supplied via env vars instead of being committed to the config file.
    config = yaml.safe_load(os.path.expandvars(raw_text))

    vast = config["vast"]
    vast_url = vast["url"].rstrip("/").removesuffix("/ws/bot") + "/ws/bot"
    bots = config.get("bots", [])

    if not bots:
        log.error("No bots configured in %s", config_path)
        sys.exit(1)

    log.info("Starting connector: %d bot(s), VAST=%s", len(bots), vast_url)

    tasks = [asyncio.create_task(handle_bot(cfg, vast_url)) for cfg in bots]

    # Graceful shutdown on SIGTERM / SIGINT
    loop = asyncio.get_running_loop()
    stop = asyncio.Event()

    def shutdown():
        log.info("Shutting down...")
        stop.set()
        for t in tasks:
            t.cancel()

    for sig in (signal.SIGINT, signal.SIGTERM):
        try:
            loop.add_signal_handler(sig, shutdown)
        except NotImplementedError:
            pass  # Windows

    try:
        await stop.wait()
    except asyncio.CancelledError:
        pass


if __name__ == "__main__":
    import argparse

    p = argparse.ArgumentParser(description="VAST Bot Connector")
    p.add_argument("--config", default="bot-connector.yaml", help="Path to config file")
    args = p.parse_args()

    try:
        asyncio.run(main(args.config))
    except KeyboardInterrupt:
        log.info("Interrupted")
