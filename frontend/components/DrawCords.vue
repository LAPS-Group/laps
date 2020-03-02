<template>
  
  <!--Loops through all given coordinates and places a red dot-->
  <div id="draw-cordinates">
    <div v-for="(point, index) in recivedCoordinates.points" :key="index">
      <canvas
        width="5"
        height="5"
        v-bind:style="{
          top: recivedCoordinates.points[index].x + 'px',
          left: recivedCoordinates.points[index].y + 'px',
          backgroundColor: colour,
          Zindex: 1
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
    }
   },
  props: [],

  data: function() {
    return {
      colour: "red",
     
    };
  }
};
</script>
