"""MCP server entry point for hw-test-runner."""

import asyncio
import argparse
import logging

from mcp.server.stdio import stdio_server

from .server import create_server

logger = logging.getLogger(__name__)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Hardware Test Runner MCP Server")
    parser.add_argument(
        "--log-level",
        default="info",
        choices=["debug", "info", "warning", "error"],
    )
    parser.add_argument("--log-file", default=None, help="Log to file instead of stderr")
    return parser.parse_args()


async def _run():
    args = parse_args()

    handlers = {"": logging.StreamHandler()}
    if args.log_file:
        handlers = {"": logging.FileHandler(args.log_file)}

    logging.basicConfig(
        level=getattr(logging, args.log_level.upper()),
        format="%(asctime)s %(name)s %(levelname)s %(message)s",
        handlers=list(handlers.values()),
    )

    server = create_server()
    init_options = server.create_initialization_options()

    async with stdio_server() as (read_stream, write_stream):
        logger.info("Hardware Test Runner MCP server starting")
        await server.run(read_stream, write_stream, init_options)


def main():
    asyncio.run(_run())


if __name__ == "__main__":
    main()
