import QtQuick

// A picture in two tones of the palette, for pictures from outside such as album art in the mono icon style: its
// dark parts in shadowColor, its light parts in lightColor, and its transparency kept.
ShaderEffect {
    // An Image, or any other item that provides a texture; it is usually hidden, since this draws it.
    property Item source
    property color shadowColor: Theme.ground
    property color lightColor: Theme.signal
    // The share of the source's texture to draw, about its centre, in each direction. A texture is always whole, so a
    // source that crops, such as an Image with PreserveAspectCrop, passes its size over its painted size.
    property vector2d uvScale: Qt.vector2d(1, 1)

    fragmentShader: Qt.resolvedUrl("shaders/duotone.frag.qsb")
}
