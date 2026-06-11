import asyncio
import logging
import sys

import click

from kaya_server.db import Database
from kaya_server.auth import hash_passkey
from kaya_server.connection_manager import ConnectionManager
from kaya_server.ws_handler import WebSocketHandler
from kaya_transfer_hub.server import MCPServer
from kaya_transfer_hub.mcp_agent import McpAgent

logger = logging.getLogger(__name__)


@click.group()
@click.option("--db-path", default=None, help="SQLite database path (default: ~/.kaya-transfer-hub/hub.db)")
@click.option("--verbose", is_flag=True, help="Enable verbose logging")
@click.pass_context
def cli(ctx, db_path, verbose):
    """File Transfer Hub — MCP tool for LLM-to-client file pushing."""
    level = logging.DEBUG if verbose else logging.INFO
    logging.basicConfig(
        level=level,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )
    ctx.ensure_object(dict)
    ctx.obj["db_path"] = db_path


@cli.command()
@click.option("--ws-port", default=9765, help="WebSocket server port (default: 9765)")
@click.option("--ws-host", default="0.0.0.0", help="WebSocket listen address (default: 0.0.0.0)")
@click.pass_context
def serve(ctx, ws_port, ws_host):
    """Start the server: run both MCP stdio and WebSocket services."""
    db_path = ctx.obj["db_path"]

    db = Database(db_path)
    db.initialize()
    cm = ConnectionManager()
    ws_handler = WebSocketHandler(db, cm, host=ws_host, port=ws_port)
    mcp_server = MCPServer(db, cm, ws_handler)

    async def _run():
        await ws_handler.start()
        logger.info(f"WebSocket server running on ws://{ws_host}:{ws_port}")
        logger.info("MCP server running on stdio")
        await mcp_server.run_stdio()

    try:
        asyncio.run(_run())
    except KeyboardInterrupt:
        logger.info("Shutting down...")
    finally:
        db.close()


@cli.command()
def mcp():
    """Start MCP stdio server only (no port binding). Delegates to run_and_send.py via Unix socket."""
    agent = McpAgent()
    try:
        asyncio.run(agent.run_stdio())
    except KeyboardInterrupt:
        pass


@cli.command()
@click.argument("client_id")
@click.argument("description")
@click.argument("passkey")
@click.pass_context
def register_client(ctx, client_id, description, passkey):
    """Pre-register a client with its ID, description, and passkey."""
    db = Database(ctx.obj["db_path"])
    db.initialize()
    try:
        if db.client_exists(client_id):
            click.echo(f"❌ Client {client_id} already exists")
            sys.exit(1)
        passkey_hash = hash_passkey(passkey)
        db.register_client(client_id, description, passkey_hash)
        click.echo(f"✅ Client {client_id} ({description}) registered successfully")
    finally:
        db.close()


@cli.command()
@click.pass_context
def list_clients(ctx):
    """List all registered clients."""
    db = Database(ctx.obj["db_path"])
    db.initialize()
    try:
        clients = db.list_clients()
        if not clients:
            click.echo("(No registered clients)")
            return
        click.echo(f"{'Client ID':20s} {'Description':30s}")
        click.echo("-" * 50)
        for c in clients:
            click.echo(f"{c.client_id:20s} {c.description}")
    finally:
        db.close()


@cli.command()
@click.argument("client_id")
@click.pass_context
def remove_client(ctx, client_id):
    """Remove a registered client by ID."""
    db = Database(ctx.obj["db_path"])
    db.initialize()
    try:
        if db.remove_client(client_id):
            click.echo(f"✅ Client {client_id} removed")
        else:
            click.echo(f"❌ Client {client_id} not found")
    finally:
        db.close()
