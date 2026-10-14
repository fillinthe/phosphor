fn main() {
    for src in [
        "#version 450\nlayout(binding=3) uniform sampler2D tex0;\nlayout(location=0) out vec4 o;\nvoid main(){ o = texture(tex0, vec2(0.5)); }\n",
        "#version 450\nuniform sampler2D tex0;\nlayout(location=0) out vec4 o;\nvoid main(){ o = texture(tex0, vec2(0.5)); }\n",
        "#version 450\n#define X 1\nlayout(binding=3) uniform sampler2D tex0;\nlayout(location=0) out vec4 o;\nvoid main(){ o = texture(tex0, vec2(0.5)); }\n",
    ] { println!("=====\n{}", phosphor_isf::proto(src)); }
}
