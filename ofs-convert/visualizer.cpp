#include "visualizer.h"
#include <stdio.h>
#include <memory.h>
#include <math.h>

const char* type_names[] = {
    #define ENTRY(name, color) #name,
    #include "visualizer_types.h"
    #undef ENTRY
};

const char* type_colors[] = {
    #define ENTRY(name, color) color,
    #include "visualizer_types.h"
    #undef ENTRY
};

BlockRange* block_range = NULL;
uint32_t resettled = 0, tag_count = 0, fragment_count = 0, pages_allocated = 0, archiver_pages = 0, group_header_pages = 0;

void visualizer_add_allocated_extent(const fat_extent& extent) {
#ifdef VISUALIZER
    pages_allocated += extent.length;
#endif  // VISUALIZER
}

void visualizer_add_tag(uint64_t tag) {
#ifdef VISUALIZER
    ++tag_count;
#endif  // VISUALIZER
}

void visualizer_add_block_range(BlockRange source) {
#ifdef VISUALIZER
    BlockRange* destination = (BlockRange*)malloc(sizeof(BlockRange));
    memcpy(destination, &source, sizeof(BlockRange));
    destination->next = block_range;
    block_range = destination;
    if(block_range->type == BlockRange::ResettledPayload)
        resettled += block_range->length;

    switch (block_range->type) {
        case BlockRange::StreamArchiverPage:
            archiver_pages += source.length;
            break;
        case BlockRange::BlockGroupHeader:
            group_header_pages += source.length;
            break;
        case BlockRange::ResettledPayload:
        case BlockRange::OriginalPayload:
            ++fragment_count;
            break;
        default:
            break;
    }
#endif  // VISUALIZER
}

void visualizer_render_to_file(const char* path, uint32_t block_count) {
#ifdef VISUALIZER
    const uint32_t line_width = 2048, line_height = 20, line_count = 55;
    FILE* output = fopen(path, "w+");
    if(!output)
        return;
    fputs("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n", output);
    fputs("<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\">\n", output);
    fprintf(output, "<svg viewBox=\"0 0 %d %d\" version=\"1.1\" xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" xml:space=\"preserve\">\n\t<g>\n", line_width, line_height*(line_count+1)+20);
    for(uint32_t i = 0; i < line_count; ++i)
        fprintf(output, "\t\t<path stroke-width=\"2\" stroke-dasharray=\"5 5\" stroke=\"grey\" d=\"M0,%fH%d\"/>\n", line_height*(i+0.4), line_width);
    fputs("\t</g>\n\t<g shape-rendering=\"crispEdges\">\n", output);
    while(block_range) {
        float begin = (float)line_width*line_count*block_range->begin/block_count,
              length = (float)line_width*line_count*block_range->length/block_count;
        uint32_t line = begin/line_width;
        begin -= line*line_width;

        while(true) {
            fprintf(output, "\t\t<rect x=\"%f\" y=\"%d\" width=\"%f\" height=\"%f\" fill=\"%s\" class=\"tag%llu\"/>\n",
                begin,
                line_height*line,
                fmin(length, line_width-begin),
                line_height*0.8,
                type_colors[block_range->type],
                block_range->tag
            );

            if(begin+length <= line_width)
                break;
            length -= fmin(length, line_width-begin);
            begin = 0;
            ++line;
        }

        block_range = block_range->next;
    }
    fputs("\t</g>\n\t<g>\n", output);
    for(uint32_t type = 0; type < sizeof(type_names)/sizeof(type_names[0]); ++type) {
        uint32_t x = 250*type+5, y = line_height*line_count;
        fprintf(output, "\t\t<rect x=\"%d\" y=\"%d\" width=\"%f\" height=\"%f\" fill=\"%s\"/>\n", x, y, line_height*0.8, line_height*0.8, type_colors[type]);
        fprintf(output, "\t\t<text x=\"%d\" y=\"%d\" font-family=\"Verdana\">%s</text>\n", x+line_height, y+15, type_names[type]);
    }
    fprintf(output, "\t\t<text x=\"5\" y=\"%d\" font-family=\"Verdana\">Blocks: %d x %d, Fragmentation: %d / %d, Pages allocated: %d (%d resettled, %d for archiver, %d for ext4 structures), Group headers: %d</text>\n", line_height*(line_count+1)+15, block_count/line_count, line_count, fragment_count, tag_count, pages_allocated, resettled, archiver_pages, pages_allocated - resettled - archiver_pages, group_header_pages);
    fputs("\t</g>\n\t<script type=\"text/javascript\" xlink:href=\"visualizer.js\"/>\n", output);
    fputs("</svg>\n", output);
    fclose(output);
#endif  // VISUALIZER
}
