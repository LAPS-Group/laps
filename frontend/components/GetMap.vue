<!--
//frontend/components/GetMap.vue: Controls the UI, request and display maps.
//Author: Even T Røraas
//Copyright (c) 2020 LAPS Group
//Distributed under the zlib licence, see LICENCE.
-->

<template>
  <div id="getMap">
    <!-- add an input field and adds it vue reactive elements-->
    <br />

    Select Map
    <br />
    <!-- creates a dropdown menu for maps -->
    <template v-if="mapMenuRender == true">
      <select v-model="selected" @change="onChange($event)" class="drop-down">
        <option v-for="option in options" v-bind:value="option.value">
          {{ option.text }}
        </option>
      </select>
    </template>
    <!--<input v-model="map_id" @change="getMap" />-->
    <br />
    <!-- calls a function to display the map-->
    <!--<button v-on:click="getMap">Get Map</button>-->

    <!-- creates a new template wich is only displayed if a map is recived-->

    <template v-if="pictureRecived == true" style="z-index=1"
      >

      <div
        ref="map"
        style="
           {
            align-items: flex-start;
            position: absolute;
            float: left;
            left: 300px;
          }
        "
      >
        <div class="mapcontainer">
          <img
            :src="this.map_link"
            v-on:click="placeMarker"
            style="z-index=0"
          />
        </div>

        <template v-if="displayM1 == true">
          <img
            :src="images.marker1"
            style="width: 2%; height: 2%; position: absolute; z-index=2"
            v-bind:style="{
              left: x1 - 14 + 'px',
              top: y1 - 24 + 'px',
              Zindex: 2,
            }"
        /></template>

        <template v-if="displayM2 == true">
          <img
            :src="images.marker1"
            style="width: 2%; height: 2%; position: absolute; z-index=2"
            v-bind:style="{
              left: x2 - 14 + 'px',
              top: y2 - 24 + 'px',
              Zindex: 2,
            }"
        /></template>
        <!-- Calls the component DrawCords-->
        <draw-cordinates />
      </div>
    </template>
  </div>
</template>
<script>
import DrawCordinates from "./DrawCords.vue";
import axios from "axios";
import { getRoute } from "route";
import { store, mutations } from "../store.js";
import marker from "images/marker1.png";

export default {
  //defines components used
  components: {
    DrawCordinates,
  },

  data: function () {
    //defines variables in vue reactive element.
    return {
      x1: "10",
      y1: "100",
      x2: "1",
      y2: "1",
      displayM1: false,
      displayM2: false,
      pictureRecived: false,
      map: null,
      map_id: null,
      map_path: "/map/",
      map_link: "",
      selectedMarker: 0,
      mapMenuRender: false,
      mapList: null,
      options: [],
      selected: "PLaceholder",
      images: {
        marker1: marker,
      },
    };
  },

  methods: {
    // displays the map based on ID
    getMap: function () {
      this.map_link = getRoute(this.map_path + this.map_id);

      // Is set to true so that, it doesn't try display maps without a map
      this.pictureRecived = true;
      // send map ID to the store
      mutations.setmap_id(this.map_id);
    },
    //run after a new option is selected in the dropdown menu
    onChange(event) {
      //takes the selected option and generate the request path
      this.map_link = getRoute(
        this.map_path + this.mapList.data.maps[this.selected]
      );
      //send the map id to the store to be used by other places that needs it
      mutations.setmap_id(this.mapList.data.maps[this.selected]);
      this.pictureRecived = true;
    },
    placeMarker(event) {
      var rect = event.target.getBoundingClientRect();
      if (this.selectedMarker == 0) {
        this.x1 = event.clientX - rect.x;
        this.y1 = event.clientY - rect.y;
        this.selectedMarker = 1;
        this.displayM1 = true;
        mutations.setMarker(this.x1, this.y1, 0);
      } else if (this.selectedMarker == 1) {
        this.x2 = event.clientX - rect.x;
        this.y2 = event.clientY - rect.y;
        mutations.setMarker(this.x2, this.y2, 1);
        this.selectedMarker = 0;
        this.displayM2 = true;
      }
    },
  },
  mounted: async function () {
    //request for all available maps
    this.mapList = await axios.get(getRoute("/maps"));
    console.log(JSON.stringify(this.mapList.data.maps[0]));
    //Places all recived maps into options that is the options in the dropdown menu
    for (let i = 0; i < this.mapList.data.maps.length; i++) {
      this.options.push({ text: this.mapList.data.maps[i], value: i });
      console.log(i);
    }
    this.mapMenuRender = true;
  },
};
</script>
<style>
#getMap {
  font-size: 18px;
  font-family: "Roboto", sans-serif;
}
canvas {
  align-items: flex-start;
  position: absolute;
  top: 0px;
  left: 0px;
  z-index: 1;
}
.mapcontainer {
  position: relative;
  align-items: flex-start;
  z-index: 0;
}

.drop-down {
  display: block;
 
  width: 175px;
 
}
</style>
