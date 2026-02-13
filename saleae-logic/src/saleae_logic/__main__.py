import asyncio
import logging

from mcp.server.stdio import stdio_server

from .config import parse_args
from .server import create_server


async def _run():
    config = parse_args()
    logging.basicConfig(
        level=getattr(logging, config.log_level.upper(), logging.INFO),
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )
    server = create_server(config)
    init_options = server.create_initialization_options()
    async with stdio_server() as (read_stream, write_stream):
        await server.run(read_stream, write_stream, init_options)


def main():
    asyncio.run(_run())


if __name__ == "__main__":
    main()
