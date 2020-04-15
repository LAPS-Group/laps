import redis
import json
import signal, sys
import time
from datetime import datetime

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

args = parser.parse_args()


# Use a global variable to keep track of whether we're running or not.
# This is required in order to handle the signals properly.

g_running = True

class RunnerException(Exception):
    pass

class Runner:
    def __init__(self):
        self.name = args.name
        self.version = args.version
        # Redis-py does connection pooling by default
        self.redis = redis.StrictRedis(host=args.redis_host, port=args.port)

        # Register module
        self.test_mode = args.test
        # set the correct log key in test mode
        if self.test_mode:
            self.log_key = "laps.testing.moduleLogs"
        else:
            self.log_key = "laps.moduleLogs"

        self.register_module()

        self.job_key = self.create_redis_key("work")


    def __enter__(self):
        return self

    # Handle module shutdown on scope exit
    def __exit__(self, _exc_type, _exc_value, _traceback):
        self.redis.rpush(
            self.create_backend_redis_key("module-shutdown"),
            self.ident
        )

    # Register a module with Redis, can throw an error
    def register_module(self):
        # For checking if a module exists, it has to be serialized in the exact same
        # way as the backend does it, with the same spacing and all.
        # There's no good way to do this, so we have to use a format string like this.
        # This might break when changing stuff in the backend.
        ident = "{{\"name\": \"{0}\", \"version\": \"{1}\"}}".format(self.name, self.version)
        self.ident = ident

        # Prod the registered_modules set to determine if we are already registered
        key = self.create_backend_redis_key("registered_modules")
        if self.redis.sismember(key, ident):
            # We already exist, throw an error
            raise RunnerException("Already have registered a module {0} v{1}".format(self.name, self.version))

        self.redis.rpush(
            self.create_backend_redis_key("register-module"),
            ident
        )
        self.log_info("Registered as {0}:{1}".format(self.name, self.version))

    # Main module loop
    def run(self, handler):
        global g_running
        blocking = True
        # Setup a signal handler to kill the loop before the next iteration when SIGINT is sent
        def signal_handler(sig, frame):
            self.log_info("Shutdown signal received, shutting down")

            if blocking:
                sys.exit(0)
            else:
                # otherwise, set running to False and exit the loop on the next iteration
                global g_running
                g_running = False

        signal.signal(signal.SIGINT, signal_handler)

        while g_running:
            # Redispy returns the key which was popped in addition to the value
            response = self.redis.blpop(self.job_key, 0)
            blocking = False
            should_run = True

            (_, response) = response
            value = json.loads(response)
            job_id = value["job_id"]
            self.log_info("Got job {0}".format(job_id))
            try:
                (should_run, response) = handler(self, value)
            except Exception as exp:
                message = "Handler failed: type: {0} contents: {1}".format(type(exp), exp)
                self.log_error(message)
                break
            if not should_run:
                g_running = False

            # Push module result to redis
            response["job_id"] = job_id
            self.redis.lpush(
                self.create_backend_redis_key("path-results"),
                json.dumps(response)
            )
            self.log_info("Completed job {}".format(job_id))
            blocking = True

    def create_redis_key(self, name):
        prefix = "laps.runner"
        if self.test_mode:
            prefix = "laps.test.runner"
        return "{0}.{1}:{2}.{3}".format(prefix, self.name, self.version, name)

    def create_backend_redis_key(self, name):
        if self.test_mode:
            return "laps.test.backend.{}".format(name)
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
        print("[{0}Z] {1}{2}\033[0m: {3}".format(datetime.utcnow(), loglevel_to_escape(level),
                                                 level, message), flush=True)
        msg = {
            "message": message,
            "level": level,
            "module": {
                "name": self.name,
                "version": self.version
            },
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
