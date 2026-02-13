import argparse
from dataclasses import dataclass


@dataclass
class Config:
    host: str = "127.0.0.1"
    port: int = 10430
    output_dir: str = "./captures"
    log_level: str = "info"


def parse_args() -> Config:
    parser = argparse.ArgumentParser(description="Saleae Logic 2 MCP Server")
    parser.add_argument("--host", default="127.0.0.1", help="Logic 2 automation host")
    parser.add_argument("--port", type=int, default=10430, help="Logic 2 automation port")
    parser.add_argument("--output-dir", default="./captures", help="Default export directory")
    parser.add_argument("--log-level", default="info", help="Logging level")
    args = parser.parse_args()
    return Config(
        host=args.host,
        port=args.port,
        output_dir=args.output_dir,
        log_level=args.log_level,
    )
