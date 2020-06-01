<!--
//frontend/components/DrawCords.vue: Draws a path when given a set of coordinates.
//Author: Even T Røraas
//Copyright (c) 2020 LAPS Group
//Distributed under the zlib licence, see LICENCE.
-->

<template>
  <!--Loops through all given coordinates and places a red dot-->
  <div id="draw-cordinates">
    <div v-for="(point, index) in recivedCoordinates.points" :key="index">
      <canvas
        width="2"
        height="2"
        v-bind:style="{
          left: recivedCoordinates.points[index].x + 'px',
          top: recivedCoordinates.points[index].y + 'px',
          backgroundColor: colour,
          Zindex: 1,
        }"
      >
      </canvas>
    </div>
  </div>
</template>

<script>
//gets the coordinated from the store
import { store, mutations } from "../store.js";
export default {
  computed: {
    recivedCoordinates() {
      return store.recivedCoordinates;
    },
  },
  props: [],

  data: function () {
    return {
      colour: "red",
    };
  },
};
</script>
