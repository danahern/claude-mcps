import asyncio
import logging

from .config import parse_args
from .server import create_server


def main():
    config = parse_args()
    logging.basicConfig(
        level=getattr(logging, config.log_level.upper(), logging.INFO),
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )
    server = create_server(config)
    asyncio.run(server.run_stdio_async())


if __name__ == "__main__":
    main()
