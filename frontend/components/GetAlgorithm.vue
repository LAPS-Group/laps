<!-- A fair bit of code belong to the old drop down menu-->
<template>
  <div id="getAlgorithm">
    Algorithm <br />
    <template v-if="algorithmsRender == true">
      <select v-model="selected" @change="onChange($event)" class="drop-down">
        <option v-for="option in options" v-bind:value="option.value">
          {{ option.text }}
        </option>
      </select>
    </template>
  </div>
</template>
<script>
import axios from "axios";
import { getRoute } from "route";
import VueDropdown from "vue-dynamic-dropdown";
import { store, mutations } from "../store.js";

export default {
  components: {
    VueDropdown,
  },
  data: function () {
    return {
      algorithms_arr: null,
      selected_algorithms: null,
      names_arr: [],
      algorithmsRender: false,
      selected: "placeholder",
      options: [],
    };
  },

  mounted: async function () {
    //Sends a request for all available algorithms
    this.algorithms_arr = await axios.get(getRoute("/algorithms"));
    let i = 0;
    for (i = 0; i < this.algorithms_arr.data.length; i++) {
      let alg =
        this.algorithms_arr.data[i].name +
        " " +
        this.algorithms_arr.data[i].version;
      this.options.push({ text: alg, value: i });

      console.log(JSON.stringify(this.options[i]));
    }
    this.algorithmsRender = true;
  },
  methods: {
    onChange(event) {
      mutations.setselected_algorithms(this.algorithms_arr.data[this.selected]);
    },
  },
};
</script>
<style>
#getAlgorithm {
  font-size: 18px;
  font-family: "Roboto", sans-serif;
}
#dropDown {
  position: absolute;
}
<<<<<<< HEAD .drop-down {
  width: 175px;
}
</style>
