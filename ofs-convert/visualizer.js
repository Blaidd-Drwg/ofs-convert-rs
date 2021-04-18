let prevColors = new Map();
function deselect() {
    for(const [rect, color] of prevColors)
        rect.setAttribute('fill', color);
    prevColors.clear();
}
document.children[0].onclick = deselect;
for(const rect of document.getElementsByTagName('rect')) {
    const tag = rect.getAttribute('class');
    if(tag === 'tag0')
        continue;
    const group = document.getElementsByClassName(tag);
    rect.onclick = function(event) {
        deselect();
        for(const rect of group) {
            const color = window.getComputedStyle(rect).fill.substr(4).split(', ').map(x => parseInt(x)*0.5);
            prevColors.set(rect, rect.getAttribute('fill'));
            rect.setAttribute('fill', 'rgb('+Math.round(color[0])+', '+Math.round(color[1])+', '+Math.round(color[2])+')');
        }
        event.stopPropagation();
    };
}
