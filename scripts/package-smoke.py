"""Exercise the installed production worker and native tools without developer PATHs."""
import argparse
from functools import partial
import hashlib
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import uuid

parser=argparse.ArgumentParser();parser.add_argument('--root',required=True);args=parser.parse_args()
root=Path(args.root).resolve();python=root/'runtime/python/python.exe';binary=root/'runtime/bin';worker=root/'worker/main.py'
environment={**os.environ,'PATH':'','PYTHONPATH':'','PYTHONHOME':'','PIP_CONFIG_FILE':'NUL'}
inventory=json.loads((root/'runtime/inventory.json').read_text('utf-8'))
for name,record in inventory['binaries'].items():
    path=root/'runtime'/record['path']
    assert hashlib.sha256(path.read_bytes()).hexdigest()==record['sha256'],name
subprocess.run([str(python),'-I','-m','pip','--isolated','--version'],env=environment,check=True,timeout=20)

def invoke(command,payload,expect_success=True):
    attempt=str(uuid.uuid4())
    request={'protocol':1,'job_id':'installed-smoke','attempt_id':attempt,'command':command,'payload':{**payload,'bin':str(binary)}}
    result=subprocess.run([str(python),'-I','-u',str(worker)],input=json.dumps(request).encode(),capture_output=True,env=environment,timeout=60)
    events=[json.loads(line) for line in result.stdout.splitlines()]
    assert events and all(e['job_id']=='installed-smoke' and e['attempt_id']==attempt for e in events),events
    terminal=events[-1]
    if expect_success:
        assert result.returncode==0 and terminal['event']=='result',terminal
    return terminal['data']

health=invoke('healthcheck',{})
assert Path(health['executable']).resolve()==python

class Handler(SimpleHTTPRequestHandler):
    def log_message(self,*args):pass

with tempfile.TemporaryDirectory(prefix='Frame smoke è ') as directory:
    temporary=Path(directory);fixture=temporary/'source';fixture.mkdir()
    subprocess.run([str(binary/'ffmpeg.exe'),'-nostdin','-v','error','-f','lavfi','-i','testsrc=size=160x90:rate=10','-f','lavfi','-i','sine=frequency=440','-t','1','-c:v','mpeg4','-pix_fmt','yuv420p','-c:a','aac','-f','dash','fixture.mpd'],cwd=fixture,env=environment,check=True,timeout=30)
    server=ThreadingHTTPServer(('127.0.0.1',0),partial(Handler,directory=str(fixture)));thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
    try:
        url=f'http://127.0.0.1:{server.server_port}/fixture.mpd'
        info=invoke('inspect',{'url':url,'options':{}})
        assert info['formats']
        for audio,codec in ((False,'original'),(True,'original'),(True,'mp3')):
            work=temporary/f'output-{audio}-{codec}';work.mkdir()
            payload={'url':url,'directory':str(work),'options':{'audio_only':audio,'audio_format':codec,'audio_bitrate':192,'fragments':2}}
            transferred=invoke('download',payload)
            assert len(transferred['files'])==(1 if audio else 2)
            verified=invoke('download',{**payload,**transferred,'phase':'finalize'})
            streams=[s['codec_type'] for s in verified['probe']['streams']]
            assert 'audio' in streams and ('video' in streams)==(not audio)
    finally:
        server.shutdown();server.server_close();thread.join()

# An absent bundled binary must fail, even on a machine with global FFmpeg.
ffmpeg=binary/'ffmpeg.exe';disabled=binary/'ffmpeg.exe.smoke-disabled'
ffmpeg.rename(disabled)
try:
    failure=invoke('inspect',{'url':'https://youtu.be/el3E4MbxRqQ','options':{}},False)
    assert failure['code']=='runtime',failure
finally:disabled.rename(ffmpeg)
print('Installed production worker: Python/pip, separate downloads, mux, audio, codecs and missing-tool checks passed.')
