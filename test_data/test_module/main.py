#!/usr/bin/env python

import laps

def handle(runner, job):
    (start_x, start_y) = (job["start"]["x"], job["start"]["y"])
    (stop_x, stop_y) = (job["stop"]["x"], job["stop"]["y"])
    (dist_x, dist_y) = (stop_x - start_x, stop_y - start_y)

    points = []
    for i in range(round(dist_x)):
        points.append({"x": start_x + i, "y": start_y})
    for i in range(round(dist_y)):
        points.append({"x": stop_x, "y": start_y + i})

    return (True, {"points": points})

with laps.Runner() as runner:
    runner.run(handle)
