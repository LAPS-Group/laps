# laps_module_runner/laps.py: Pathfinding module library
# Author: Håkon Jordet
# Copyright (c) 2020 LAPS Group
# Distributed under the zlib licence, see LICENCE.

import redis
import json
import signal, sys
import time
from datetime import datetime
import traceback

# Re-usable command line arguments for LAPS modules. Mostly useful for the backend, and perhaps while
# testing if a module is working.
import argparse, os
parser = argparse.ArgumentParser(description='LAPS pathfinding module')
parser.add_argument('name', type=str)
parser.add_argument('version', type=str)
# Default value should be the Docker host IP
parser.add_argument('--redis_host', type=str, default='localhost', required=False)
# port to connect to redis at
parser.add_argument('--port', type=int, default='6379')
# Test mode check
parser.add_argument('--test', action='store_true')
# Worker number.
parser.add_argument('--worker_number', type=int, default=0, required=False)

args = parser.parse_args()

# Use a global variable to keep track of whether we're running or not.
# This is required in order to handle the signals properly.

g_running = True

# Class to be used when a job fails.
class JobFailure(Exception):
    pass

class Runner:
    def __init__(self):
        self.name = args.name
        self.version = args.version
        self.worker_number = args.worker_number
        # Redis-py does connection pooling by default
        self.redis = redis.StrictRedis(host=args.redis_host, port=args.port)

        self.registered = False

        # Register module
        self.test_mode = args.test
        # set the correct log key in test mode
        if self.test_mode:
            self.log_key = "laps.testing.moduleLogs"
        else:
            self.log_key = "laps.moduleLogs"

        self.job_key = self.create_redis_key("work")


    def __enter__(self):
        return self

    # Handle module shutdown on scope exit
    def __exit__(self, exc_type, exc_value, tb):
        if self.registered:
            self.redis.rpush(
                self.__create_backend_redis_key("module-shutdown"),
                self.ident
            )
        if exc_type is not SystemExit and exc_type is not None:
            if traceback is not None:
                string = ''.join(traceback.format_exception(exc_type, exc_value, tb))
                self.log_error(string)
            

    # Get the map data from a job. 
    def get_map_data(self, job):
        data = self.redis.hget("laps.mapdata.image", job["map_id"])
        if data is None:
            raise JobFailure("Map {} is missing!".format(job["map_id"]))
        return data

    def get_map_metadata(self, job):
        data = self.redis.hget("laps.mapdata.meta", job["map_id"])
        if data is None:
            raise JobFailure("Map {} metadata is missing!".format(job["map_id"]))
        return json.loads(data)

    # Register self as a module in the system.
    def __register_module(self):
        ident = json.dumps({
            "name": self.name,
            "version": self.version
        })
        self.ident = ident

        self.redis.rpush(
            self.__create_backend_redis_key("register-module"),
            ident
        )
        self.log_info("Registered as {0}:{1}".format(self.name, self.version))
        self.registered = True

    # Main module loop
    def run(self, handler):
        # Register self here as ready to accept jobs.
        self.__register_module()

        global g_running
        blocking = True
        # Setup a signal handler to kill the loop before the next iteration when SIGINT is sent
        def signal_handler(sig, frame):
            self.log_info("Shutdown signal received, shutting down")

            # If we're just sitting around waiting for a job we can just exit immediately.
            # Otherwise we would have to receive a job and only then be able to exit.
            if blocking:
                sys.exit(0)
            else:
                # otherwise, set running to False and exit the loop on the next iteration
                global g_running
                g_running = False

        signal.signal(signal.SIGINT, signal_handler)

        while g_running:
            try:
                # Redispy returns the key which was popped in addition to the value
                (_, job) = self.redis.blpop(self.job_key, 0)
                blocking = False
                should_run = True

                #Run the handler function
                value = json.loads(job)
                job_id = value["job_id"]
                self.log_info("Got job {0}".format(job_id))
                # This will throw some kind of exception if things go wrong
                result = handler(self, value)

                # Send the result to the backend.
                response = {
                    "job_id": job_id,
                    "outcome": "success",
                    "points": result
                }
                self.redis.lpush(
                    self.__create_backend_redis_key("path-results"),
                    json.dumps(response)
                )
                self.log_info("Completed job {}".format(job_id))
                blocking = True

            except JobFailure as exp:
                # A manually triggered failure condition, intentionally done by the module developer.
                # Considered a recoverable error.
                message = "Job {0} failed: {1}".format(job_id, exp)
                self.log_error(message)
                self.__fail_job(job_id)

            except Exception as exp:
                # An unexpected failure from the module
                # Fail the job and rethrow the exception
                self.__fail_job(job_id)
                raise exp

    def __fail_job(self, job_id):
        message = {"job_id": job_id, "outcome": "failure"}
        self.redis.lpush(self.__create_backend_redis_key("path-results"), json.dumps(message))

    def create_redis_key(self, name):
        prefix = "laps.runner"
        if self.test_mode:
            prefix = "laps.testing.runner"
        return "{0}.{1}:{2}.{3}".format(prefix, self.name, self.version, name)

    def __create_backend_redis_key(self, name):
        if self.test_mode:
            return "laps.testing.backend.{}".format(name)
        else:
            return "laps.backend.{}".format(name)

    def __log(self, level, message):
        def loglevel_to_escape(level):
            if level == "debug":
                return "\033[37m"
            elif level == "warn":
                return "\033[33m"
            elif level == "error":
                return "\033[31m"
            elif level == "info":
                return "\033[32m"
            else:
                return "\033[36m"
        print("[{0}Z {1}{2}\033[0m]: {3}".format(datetime.utcnow(), loglevel_to_escape(level),
                                                 level, message), flush=True)
        msg = {
            "message": message,
            "level": level,
            "module": {
                "name": self.name,
                "version": self.version
            },
            "worker": self.worker_number,
            "instant": int(time.time())
        }

        self.redis.rpush(self.log_key, json.dumps(msg))

    # Return a runtime error in the module
    def log_error(self, message):
        self.__log("error", message)
    def log_info(self, message):
        self.__log("info", message)
    def log_debug(self, message):
        self.__log("debug", message)
    def log_warn(self, message):
        self.__log("warn", message)
