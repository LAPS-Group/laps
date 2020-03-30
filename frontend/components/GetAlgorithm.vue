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
    this.algorithms_arr = await axios.get(getRoute("/algorithms"));
    console.log(this.algorithms_arr);

    let i;
    for (i = 0; i < this.algorithms_arr.data.length; i++) {
      this.config.options[i].value = this.algorithms_arr.data[i].name;
      this.names_arr[i] = this.algorithms_arr.data[i].name;
    }
    //this.selected_algorithms = this.algorithms_arr.data;
  },
  methods: {
    setNewSelectedOption(selectedOption) {
      this.config.placeholder = selectedOption.value;

      //console.log(this.algorithms_arr);
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
