import argparse
import asyncio

from .server import serve


def main() -> None:
    parser = argparse.ArgumentParser(prog="lico_host", description=__doc__)
    parser.add_argument("--socket", required=True, help="path inside an owner-only directory")
    parser.add_argument("--sessions", type=int, default=1)
    parser.add_argument("--producing", type=int, default=1, help="sessions that emit output")
    parser.add_argument("--rate", type=float, default=100.0, help="records per second per producer")
    parser.add_argument("--history", type=int, default=5000, help="records seeded before start")
    args = parser.parse_args()
    asyncio.run(serve(args.socket, args.sessions, args.producing, args.rate, args.history))


main()
