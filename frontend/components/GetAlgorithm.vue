<template>
  <div id="getAlgorithm">
    <br />
    <div class="dropDown">
      <vue-dropdown
        :config="config"
        @setSelectedOption="setNewSelectedOption($event)"
      >
      </vue-dropdown>
    </div>
  </div>
</template>
<script>
import axios from "axios";
import { getRoute } from "route";
import VueDropdown from "vue-dynamic-dropdown";
import { store, mutations } from "../store.js";
//Dropdown menu gotten from https://vuejsexamples.com/a-highly-dynamic-vue-dropdown-component/ on 06.03.2020

export default {
  components: {
    VueDropdown
  },
  data: function() {
    return {
      algorithms_arr: null,
      selected_algorithms: null,
      names_arr: [],
      config: {
        options: [{}],
        placeholder: "Algorithm",
        backgroundColor: "#cde4f5",
        textColor: "black",
        borderRadius: "1.5em",
        border: "1px solid gray",
        width: 220
      }
    };
  },

  mounted: async function() {
    //Sends a request for all available algorithms
    this.algorithms_arr = await axios.get(getRoute("/algorithms"));
    console.log(this.algorithms_arr);

    let i;
    for (i = 0; i < this.algorithms_arr.data.length; i++) {
      //the recived algoritmhs are copied into two arrays.
      this.config.options[i].value = this.algorithms_arr.data[i].name;
      this.names_arr[i] = this.algorithms_arr.data[i].name;
      console.log(this.algorithms_arr.data[i].name);
    }
  },
  methods: {
    //run if algorithm is change and sets the new on, and sends it to the store.
    setNewSelectedOption(selectedOption) {
      //updates the placeholder i the dropdown menu
      this.config.placeholder = selectedOption.value;
      //
      let a = this.names_arr.indexOf(this.config.placeholder);
      this.selected_algorithms = this.algorithms_arr.data[a];
      mutations.setselected_algorithms(this.selected_algorithms);
    }
  }
};
</script>
<style>
#getAlgorithm {
  height: 60px;
}
#dropDown {
  position: absolute;
}
</style>
