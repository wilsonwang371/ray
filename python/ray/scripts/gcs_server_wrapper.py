#!/usr/bin/env python
import argparse
import asyncio
import logging
import os
import socket
import subprocess
import sys
import threading
import time

import redis
import redis_lock
import tornado.web
from healthcheck import EnvironmentDump, HealthCheck, TornadoHandler

START_REDIS_WAIT_RETRIES = 16
HEALTHZ_SERVER_PORT = 8080

logger = None


class GCSHealthCheck(threading.Thread):
    def __init__(self):
        self.app = tornado.web.Application()
        self.health = HealthCheck()
        self.health.add_check(self.gcs_available)
        self.envdump = EnvironmentDump()
        self.app.add_handlers(
            r".*",
            [
                (
                    "/healthz",
                    TornadoHandler, dict(checker=self.health)
                ),
                (
                    "/",
                    TornadoHandler, dict(checker=self.health)
                ),
                (
                    "/environment",
                    TornadoHandler, dict(checker=self.envdump)
                ),
            ]
        )
        super(GCSHealthCheck, self).__init__()

    def gcs_available(self):
        return True, "gcs ok"

    def run(self):
        asyncio.set_event_loop(asyncio.new_event_loop())
        self.app.listen(HEALTHZ_SERVER_PORT)
        tornado.ioloop.IOLoop.instance().start()

    def stop(self):
        tornado.ioloop.IOLoop.instance().stop()


def get_redis_leader_lock(redis_cli, gcs_instance_id):
    lock = redis_lock.Lock(
        redis_cli,
        "GcsLeaderLock",
        expire=5,
        auto_renewal=True,
        id=gcs_instance_id,
    )
    while not lock.acquire(blocking=False):
        time.sleep(3)
        logger.info("Waiting to acquire GCS leader lock")
    logger.info("GCS leader lock acquired")
    return lock


def get_redis_client(redis_ip_address, redis_port, password):
    for i in range(START_REDIS_WAIT_RETRIES):
        cli = redis.StrictRedis(
            host=redis_ip_address, port=int(redis_port), password=password
        )
        try:
            cli.ping()
            return cli
        except Exception:
            logger.warning("Failed to ping redis server. Retrying...")
            time.sleep(2)
    raise


def main():
    global logger

    rtncode = 0
    parser = argparse.ArgumentParser()
    parser.add_argument("--redis_address", type=str, default=None)
    parser.add_argument("--redis_port", type=str, default=None)
    parser.add_argument("--redis_password", type=str, default=None)
    parser.add_argument("--gcs_server_port", type=str, default=None)
    parser.add_argument("--log_dir", type=str, default=None)
    args, _ = parser.parse_known_args()
    if (
        not args.redis_address
        or not args.redis_port
        or not args.redis_password
        or not args.gcs_server_port
    ):
        raise ValueError("Invalid redis info")

    if args.log_dir is not None:
        logging.basicConfig(filename=os.path.join(args.log_dir, "gcs_server_wrapper.log"),
                            filemode='a',
                            format='%(asctime)s,%(msecs)d %(name)s %(levelname)s %(message)s',
                            datefmt='%H:%M:%S',
                            level=logging.DEBUG)

    logger = logging.getLogger(__name__)

    gcs_instance_id = "{}_{}".format(socket.gethostname(), args.gcs_server_port)

    logger.info("connecting to redis {}:{}".format(args.redis_address, args.redis_port))
    cli = get_redis_client(args.redis_address, args.redis_port, args.redis_password)
    lock = get_redis_leader_lock(cli, gcs_instance_id)

    gcs_healthz_server = GCSHealthCheck()
    gcs_healthz_server.daemon = True
    gcs_healthz_server.start()

    try:
        subprocess.run(
            sys.argv[1:],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=True,
        )
    except subprocess.CalledProcessError as e:
        rtncode = e.returncode

    gcs_healthz_server.stop()
    lock.release()
    sys.exit(rtncode)


if __name__ == "__main__":
    main()
