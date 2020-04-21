<!-- A fair bit of code belong to the old drop down menu-->
<template>
  <div id="getAlgorithm">
    <!--
    <br />
    <div class="dropDown">
      <vue-dropdown
        :config="config"
        @setSelectedOption="setNewSelectedOption($event)"
      >
      </vue-dropdown>
    </div>-->
    Algorithm <br />
    <template v-if="algorithmsRender == true">
      <select v-model="selected" @change="onChange($event)" class="drop-down">
        <option v-for="option in options" v-bind:value="option.value">
          {{ option.text }}
        </option>
      </select>
      <!--<span>Selected: {{ selected }}</span>-->
    </template>
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
      algorithmsRender: false,
      selected: "placeholder",
      options: []
      /*
      config: {
        //options: [{value:null},{value:null}],
        options: [{ value: "test1" }, { value: "test2" }],
        placeholder: "Algorithm",
        backgroundColor: "#cde4f5",
        textColor: "black",
        borderRadius: "1.5em",
        border: "1px solid gray",
        width: 220,
        
      },*/
    };
  },

  mounted: async function() {
    //Sends a request for all available algorithms
    this.algorithms_arr = await axios.get(getRoute("/algorithms"));
    console.log(this.algorithmsRender);
    let i = 0;
    for (i = 0; i < this.algorithms_arr.data.length; i++) {
      //console.log(i);
      //this.options[i].value = this.algorithms_arr.data[i].name;
      //this.$set(this.options[i].value, this.algorithms_arr.data[i].name, )

      //this.$set(this.options[i], "value", this.algorithms_arr.data[i].name);
      //this.$set(this.options[i], "text", this.algorithms_arr.data[i].name);
      this.options.push({ text: this.algorithms_arr.data[i].name, value: i });
      //console.log(this.options.value);
      //this.options[i].value =Object.assign({},this.options[i].value, this.algorithms_arr.data[i].name;)
      //this.options[i].text = this.algorithms_arr.data[i].name;
      //this.options[i].text =Object.assign({},this.options[i].text, this.algorithms_arr.data[i].name;)
      console.log(JSON.stringify(this.options[i]));
    }
    this.algorithmsRender = true;
    /*
    let i;
    for ( i = 0; i <= (this.algorithms_arr.data.length-1); i++) {
      //the recived algoritmhs are copied into two arrays.
      this.config.options[i].value = this.algorithms_arr.data[i].name;
      this.names_arr[i] = this.algorithms_arr.data[i].name;
      console.log(this.algorithms_arr.data[i].name);
      */
    /*
      for ( let i =0; i < this.algorithms_arr.data.length; i++)
      {
        console.log(i);
        console.log(JSON.stringify(this.algorithms_arr.data[i].name));
        this.config.options[i].value = this.algorithms_arr.data[i].name;
        console.log(JSON.stringify(this.config.options));
      }
*/
  },
  methods: {
    //run if algorithm is change and sets the new on, and sends it to the store.
    /*
    setNewSelectedOption(selectedOption) {
      //updates the placeholder i the dropdown menu
      this.config.placeholder = selectedOption.value;
      //
      let a = this.names_arr.indexOf(this.config.placeholder);
      this.selected_algorithms = this.algorithms_arr.data[a];
      mutations.setselected_algorithms(this.selected_algorithms);
      
    },*/
    onChange(event) {
      mutations.setselected_algorithms(this.algorithms_arr.data[this.selected]);
    }
  }
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
.drop-down {
  width: 175px;
}
</style>
