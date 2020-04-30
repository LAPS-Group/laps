#!/usr/bin/env python

import laps

def handle(runner, job):
    raise laps.JobFailure("This job failed for some reason!")

with laps.Runner() as runner:
    runner.log_error("This is an error")
    runner.log_debug("This is a debug")
    runner.log_info("This is an info")
    runner.run(handle)
