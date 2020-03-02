
const HtmlWebpackPlugin = require('html-webpack-plugin');
const path = require('path');
const VueLoaderPlugin = require('vue-loader/lib/plugin')

module.exports = {
    entry: "./frontend/index.js",
    output: {
        filename: 'main.js',
        path: path.resolve(__dirname, 'dist'),
    },
    resolve: {
        alias: {
            'vue$': 'vue/dist/vue.esm.js'
        }
    },
    module: {
        rules: [
          // ... other rules
          {test: /\.vue$/,loader: 'vue-loader'},
          { test: /\.css$/, use: ['vue-style-loader', 'css-loader']},
          {test: /\.(png|jpe?g|gif)$/i, use: [ {loader: 'file-loader',},],},
        ]
      },
    plugins: [
        new VueLoaderPlugin(),
        new HtmlWebpackPlugin({
            filename: 'index.html',
            template: 'frontend/index.html'
        })
    ]
}